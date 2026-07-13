use super::{KeyScanPage, KvDriver, RedisEntry, RedisValueDto};
use crate::error::AppError;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

pub struct RedisDriver { conn: MultiplexedConnection }

impl RedisDriver {
    pub async fn connect(host: &str, port: u16, password: Option<&str>, db_index: i64) -> Result<Self, AppError> {
        let info = redis::ConnectionInfo {
            addr: redis::ConnectionAddr::Tcp(host.to_string(), port),
            redis: redis::RedisConnectionInfo {
                db: db_index,
                username: None,
                password: password.map(String::from),
                protocol: redis::ProtocolVersion::RESP2,
            },
        };
        let client = redis::Client::open(info)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

    pub async fn connect_url(url: &str) -> Result<Self, AppError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { conn })
    }

}

#[async_trait::async_trait]
impl KvDriver for RedisDriver {
    async fn ping(&self) -> Result<(), AppError> {
        let mut c = self.conn.clone();
        redis::cmd("PING").query_async::<String>(&mut c).await?;
        Ok(())
    }

    async fn scan_keys(&self, pattern: &str, cursor: u64, count: u32) -> Result<KeyScanPage, AppError> {
        let mut c = self.conn.clone();
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor).arg("MATCH").arg(pattern).arg("COUNT").arg(count)
            .query_async(&mut c).await?;
        Ok(KeyScanPage { keys, cursor: next })
    }

    async fn get_value(&self, key: &str) -> Result<RedisEntry, AppError> {
        let mut c = self.conn.clone();
        let ttl: i64 = c.ttl(key).await?;
        let kind: String = redis::cmd("TYPE").arg(key).query_async(&mut c).await?;
        if kind == "none" {
            // missing key contract: ttl -2, empty string value
            return Ok(RedisEntry { key: key.to_string(), ttl_secs: -2, value: RedisValueDto::String { value: String::new() }, total_items: None });
        }
        // capped types report the full length so the UI can refuse lossy edits
        let (value, total_items) = match kind.as_str() {
            "string" => (RedisValueDto::String { value: c.get(key).await? }, None),
            "hash" => {
                let map: Vec<(String, String)> = redis::cmd("HGETALL").arg(key).query_async(&mut c).await?;
                (RedisValueDto::Hash { fields: map }, None)
            }
            "list" => {
                let total: i64 = c.llen(key).await?;
                (RedisValueDto::List { items: c.lrange(key, 0, 999).await? }, Some(total as u64))
            }
            "set" => (RedisValueDto::Set { members: c.smembers(key).await? }, None),
            "zset" => {
                let total: i64 = c.zcard(key).await?;
                let members: Vec<(String, f64)> = c.zrange_withscores(key, 0, 999).await?;
                (RedisValueDto::ZSet { members }, Some(total as u64))
            }
            other => return Err(AppError::query(format!("unsupported redis type: {other}"))),
        };
        Ok(RedisEntry { key: key.to_string(), ttl_secs: ttl, value, total_items })
    }

    async fn set_value(&self, key: &str, value: &RedisValueDto) -> Result<(), AppError> {
        let mut c = self.conn.clone();
        let ttl: i64 = c.ttl(key).await?;
        let mut pipe = redis::pipe();
        pipe.atomic();
        pipe.del(key);
        match value {
            RedisValueDto::String { value } => { pipe.set(key, value); }
            RedisValueDto::Hash { fields } => { for (f, v) in fields { pipe.hset(key, f, v); } }
            RedisValueDto::List { items } => { for it in items { pipe.rpush(key, it); } }
            RedisValueDto::Set { members } => { for m in members { pipe.sadd(key, m); } }
            RedisValueDto::ZSet { members } => { for (m, s) in members { pipe.zadd(key, m, *s); } }
        }
        if ttl > 0 { pipe.expire(key, ttl); }
        pipe.query_async::<()>(&mut c).await?;
        Ok(())
    }

    async fn delete_key(&self, key: &str) -> Result<(), AppError> {
        let mut c = self.conn.clone();
        let _: () = c.del(key).await?;
        Ok(())
    }

    async fn set_ttl(&self, key: &str, ttl_secs: i64) -> Result<(), AppError> {
        let mut c = self.conn.clone();
        if ttl_secs > 0 {
            let _: bool = c.expire(key, ttl_secs).await?;
        } else {
            let _: bool = c.persist(key).await?;
        }
        Ok(())
    }
}
