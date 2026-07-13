mod common;
use dbclient_lib::drivers::mysql::MysqlDriver;
use dbclient_lib::drivers::SqlDriver;

#[tokio::test]
async fn mysql_batteries_and_cancel() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_MYSQL_URL") else {
        eprintln!("skipping: DBCLIENT_TEST_MYSQL_URL not set");
        return;
    };
    let d = MysqlDriver::connect_url(&url).await.unwrap();
    // seeded once; browse + query batteries share the same fixture, run sequentially
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_browse_battery(&d).await;
    common::assert_query_battery(&d).await;

    // cancel: stream SLEEP(10) in a task, kill it, expect return well before 10s
    let mut s = d.acquire_session().await.unwrap();
    let sid = s.session_id().expect("mysql session id");
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let handle = tokio::spawn(async move { s.stream("SELECT SLEEP(10)", tx).await });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let t0 = std::time::Instant::now();
    d.kill_session(&sid).await.unwrap();
    let _ = handle.await.unwrap(); // Ok (SLEEP returns 1 when killed) or QueryError -- either is fine
    assert!(t0.elapsed() < std::time::Duration::from_secs(5), "kill did not interrupt");

    // edit battery mutates the table: re-seed first, run last
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_edit_battery(&d).await;
    common::assert_structure_battery(&d).await;

    // transactions need a fresh seed and one session per scenario
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_txn_battery(&d).await;
}

/// regression (db-app-explorer round 2 B1): DECIMAL/TIME must decode, not fake-NULL
#[tokio::test]
async fn mysql_type_zoo_decodes() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_MYSQL_URL") else { return };
    let d = MysqlDriver::connect_url(&url).await.unwrap();
    d.execute_raw("DROP TABLE IF EXISTS zoo_my").await.unwrap();
    d.execute_raw(
        "CREATE TABLE zoo_my (id int primary key, price decimal(10,2), tm time, ts timestamp, big bigint unsigned)",
    ).await.unwrap();
    d.execute_raw(
        "INSERT INTO zoo_my VALUES (1, 19.99, '13:14:15', '2026-01-02 03:04:05', 18446744073709551615)",
    ).await.unwrap();

    let t = dbclient_lib::drivers::TableInfo {
        schema: None, name: "zoo_my".into(), kind: dbclient_lib::drivers::TableKind::Table,
    };
    let page = d.fetch_rows(&t, &dbclient_lib::drivers::FetchOptions {
        limit: 10, offset: 0, sort: None, filters: vec![],
    }).await.unwrap();
    let row = &page.rows[0];
    for (i, cell) in row.iter().enumerate() {
        assert!(!cell.is_null(), "column {} ({}) decoded as NULL", i, page.columns[i].name);
    }
    let by_name = |n: &str| {
        let i = page.columns.iter().position(|c| c.name == n).unwrap();
        &row[i]
    };
    assert_eq!(by_name("price"), &serde_json::json!("19.99"));
    assert_eq!(by_name("big"), &serde_json::json!("18446744073709551615"));
    d.execute_raw("DROP TABLE zoo_my").await.unwrap();
}
