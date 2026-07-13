use dbclient_lib::drivers::redis_kv::RedisDriver;
use dbclient_lib::drivers::{KvDriver, RedisValueDto};
use redis::AsyncCommands;

/// Test-only fixture seeding (FLUSHDB is never compiled into the shipping app).
async fn flush_and_seed(url: &str) {
    let client = redis::Client::open(url).unwrap();
    let mut c = client.get_multiplexed_async_connection().await.unwrap();
    redis::cmd("FLUSHDB").query_async::<()>(&mut c).await.unwrap();
    let _: () = c.set("fixture:str", "hello").await.unwrap();
    let _: () = c.hset_multiple("fixture:hash", &[("f1", "v1"), ("f2", "v2")]).await.unwrap();
    let _: () = c.rpush("fixture:list", &["a", "b"]).await.unwrap();
    let _: () = c.zadd_multiple("fixture:zset", &[(1.0, "one"), (2.0, "two")]).await.unwrap();
}

#[tokio::test]
async fn redis_scan_and_typed_values() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_REDIS_URL") else {
        eprintln!("skipping: DBCLIENT_TEST_REDIS_URL not set");
        return;
    };
    flush_and_seed(&url).await;
    let d = RedisDriver::connect_url(&url).await.unwrap();

    // scan to exhaustion with a pattern
    let mut keys = vec![];
    let mut cursor = 0u64;
    loop {
        let page = d.scan_keys("fixture:*", cursor, 100).await.unwrap();
        keys.extend(page.keys);
        cursor = page.cursor;
        if cursor == 0 { break; }
    }
    assert_eq!(keys.len(), 4);

    let s = d.get_value("fixture:str").await.unwrap();
    assert_eq!(s.value, RedisValueDto::String { value: "hello".into() });
    assert_eq!(s.ttl_secs, -1);

    let h = d.get_value("fixture:hash").await.unwrap();
    assert!(matches!(h.value, RedisValueDto::Hash { ref fields } if fields.len() == 2));

    let l = d.get_value("fixture:list").await.unwrap();
    assert!(matches!(l.value, RedisValueDto::List { ref items } if items == &vec!["a".to_string(), "b".to_string()]));

    let z = d.get_value("fixture:zset").await.unwrap();
    assert!(matches!(z.value, RedisValueDto::ZSet { ref members } if members.len() == 2));

    // write ops share the same db: run sequentially after the read assertions
    write_ops(&d, &url).await;

    // regression (db-app-explorer B1): capped types must report their full length
    truncation_reporting(&d, &url).await;
}

async fn truncation_reporting(d: &RedisDriver, url: &str) {
    let client = redis::Client::open(url).unwrap();
    let mut c = client.get_multiplexed_async_connection().await.unwrap();
    let _: () = c.del("fixture:big").await.unwrap();
    let items: Vec<i32> = (0..1500).collect();
    for chunk in items.chunks(500) {
        let _: () = c.rpush("fixture:big", chunk).await.unwrap();
    }
    let e = d.get_value("fixture:big").await.unwrap();
    assert_eq!(e.total_items, Some(1500), "list must report full length");
    assert!(matches!(e.value, RedisValueDto::List { ref items } if items.len() == 1000));

    // small/uncapped values carry no partial view
    let small = d.get_value("fixture:list").await.unwrap();
    assert_eq!(small.total_items, Some(3));
    let s = d.get_value("fixture:str").await.unwrap();
    assert_eq!(s.total_items, None);
    let _: () = c.del("fixture:big").await.unwrap();
}

async fn write_ops(d: &RedisDriver, url: &str) {
    flush_and_seed(url).await;

    // string rewrite preserves TTL
    d.set_ttl("fixture:str", 3600).await.unwrap();
    d.set_value("fixture:str", &RedisValueDto::String { value: "world".into() }).await.unwrap();
    let e = d.get_value("fixture:str").await.unwrap();
    assert_eq!(e.value, RedisValueDto::String { value: "world".into() });
    assert!(e.ttl_secs > 3500, "ttl lost on rewrite: {}", e.ttl_secs);

    // type change: string key becomes a hash
    d.set_value("fixture:str", &RedisValueDto::Hash { fields: vec![("a".into(), "1".into())] }).await.unwrap();
    assert!(matches!(d.get_value("fixture:str").await.unwrap().value, RedisValueDto::Hash { .. }));

    // list rewrite
    d.set_value("fixture:list", &RedisValueDto::List { items: vec!["x".into(), "y".into(), "z".into()] }).await.unwrap();
    assert!(matches!(d.get_value("fixture:list").await.unwrap().value, RedisValueDto::List { ref items } if items.len() == 3));

    // zset rewrite
    d.set_value("fixture:zset", &RedisValueDto::ZSet { members: vec![("solo".into(), 9.5)] }).await.unwrap();

    // persist + delete
    d.set_ttl("fixture:str", 0).await.unwrap();
    assert_eq!(d.get_value("fixture:str").await.unwrap().ttl_secs, -1);
    d.delete_key("fixture:hash").await.unwrap();
    assert_eq!(d.get_value("fixture:hash").await.unwrap().ttl_secs, -2); // missing
}
