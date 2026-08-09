use super::{KeyScanPage, KvDriver, RedisEntry, RedisValueDto};
use crate::error::AppError;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

/// Max elements pulled for a single value so a huge hash/set/list/zset can't
/// flood the wire or the DOM. The UI reports "showing N of M" and disables edit.
const REDIS_VALUE_CAP: usize = 1000;

/// SCAN-family loop that collects up to `cap` (field, value) pairs (HSCAN).
async fn scan_pairs(
    c: &mut MultiplexedConnection,
    cmd: &str,
    key: &str,
    cap: usize,
) -> Result<Vec<(String, String)>, AppError> {
    let mut cursor: u64 = 0;
    let mut out: Vec<(String, String)> = Vec::new();
    loop {
        let (next, chunk): (u64, Vec<String>) = redis::cmd(cmd)
            .arg(key).arg(cursor).arg("COUNT").arg(256)
            .query_async(c).await?;
        let mut it = chunk.into_iter();
        while let (Some(f), Some(v)) = (it.next(), it.next()) {
            out.push((f, v));
            if out.len() >= cap { return Ok(out); }
        }
        cursor = next;
        if cursor == 0 { break; }
    }
    Ok(out)
}

/// SSCAN loop that collects up to `cap` set members.
async fn scan_members(c: &mut MultiplexedConnection, key: &str, cap: usize) -> Result<Vec<String>, AppError> {
    let mut cursor: u64 = 0;
    let mut out: Vec<String> = Vec::new();
    loop {
        let (next, chunk): (u64, Vec<String>) = redis::cmd("SSCAN")
            .arg(key).arg(cursor).arg("COUNT").arg(256)
            .query_async(c).await?;
        for m in chunk {
            out.push(m);
            if out.len() >= cap { return Ok(out); }
        }
        cursor = next;
        if cursor == 0 { break; }
    }
    Ok(out)
}

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
                // HSCAN-capped so a huge hash doesn't pull everything / flood the DOM
                let total: i64 = c.hlen(key).await?;
                let fields = scan_pairs(&mut c, "HSCAN", key, REDIS_VALUE_CAP).await?;
                (RedisValueDto::Hash { fields }, Some(total as u64))
            }
            "list" => {
                let total: i64 = c.llen(key).await?;
                (RedisValueDto::List { items: c.lrange(key, 0, (REDIS_VALUE_CAP - 1) as isize).await? }, Some(total as u64))
            }
            "set" => {
                let total: i64 = c.scard(key).await?;
                let members = scan_members(&mut c, key, REDIS_VALUE_CAP).await?;
                (RedisValueDto::Set { members }, Some(total as u64))
            }
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

    async fn select_db(&self, index: i64) -> Result<(), AppError> {
        // All clones share one underlying multiplexed connection, so SELECT sticks
        // for subsequent commands until switched again.
        let mut c = self.conn.clone();
        let _: () = redis::cmd("SELECT").arg(index).query_async(&mut c).await?;
        Ok(())
    }
}

#[cfg(test)]
mod select_db_test {
    use super::*;
    use crate::drivers::KvDriver;

    // cargo test --lib redis_kv::select_db_test -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn select_db_persists_across_ops() {
        let d = RedisDriver::connect("127.0.0.1", 63791, None, 0).await.expect("connect");
        // write a probe into db 5, then confirm a later op on the same driver sees it
        d.select_db(5).await.expect("select 5");
        d.set_value("tabula_seltest", &RedisValueDto::String { value: "hi".into() }).await.expect("set");
        let got = d.get_value("tabula_seltest").await.expect("get");
        assert!(matches!(got.value, RedisValueDto::String { ref value } if value == "hi"));
        // switching away then back keeps db state coherent
        d.select_db(0).await.expect("select 0");
        let missing = d.get_value("tabula_seltest").await.expect("get db0");
        assert_eq!(missing.ttl_secs, -2, "key must NOT exist in db 0");
        // cleanup
        d.select_db(5).await.unwrap();
        d.delete_key("tabula_seltest").await.unwrap();
        println!("SELECT stickiness OK");
    }
}
