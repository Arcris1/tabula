mod common;
use dbclient_lib::drivers::postgres::PostgresDriver;
use dbclient_lib::drivers::SqlDriver;

#[tokio::test]
async fn postgres_batteries_and_cancel() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_PG_URL") else {
        eprintln!("skipping: DBCLIENT_TEST_PG_URL not set");
        return;
    };
    let d = PostgresDriver::connect_url(&url).await.unwrap();
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_browse_battery(&d).await;
    common::assert_query_battery(&d).await;

    let mut s = d.acquire_session().await.unwrap();
    let sid = s.session_id().expect("pg session id");
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let handle = tokio::spawn(async move { s.stream("SELECT pg_sleep(10)", tx).await });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let t0 = std::time::Instant::now();
    d.kill_session(&sid).await.unwrap();
    let res = handle.await.unwrap();
    assert!(res.is_err(), "pg cancel should surface as query error");
    assert!(t0.elapsed() < std::time::Duration::from_secs(5));

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

/// regression (db-app-explorer round 2 B1/B3): type zoo must never render fake NULLs
#[tokio::test]
async fn postgres_type_zoo_decodes() {
    let Ok(url) = std::env::var("DBCLIENT_TEST_PG_URL") else { return };
    let d = PostgresDriver::connect_url(&url).await.unwrap();
    d.execute_raw("DROP TABLE IF EXISTS zoo_pg").await.unwrap();
    d.execute_raw(
        "CREATE TABLE zoo_pg (id int primary key, n numeric(12,4), sm smallint, re real, \
         u uuid, tz timestamptz, tm time, iv interval, mo money, arr int[], big bigint)",
    ).await.unwrap();
    d.execute_raw(
        "INSERT INTO zoo_pg VALUES (1, 12345.6789, 7, 1.5, 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', \
         '2026-01-02T03:04:05Z', '13:14:15', interval '2 days', 12.34, ARRAY[1,2,3], 9007199254740993)",
    ).await.unwrap();

    let t = dbclient_lib::drivers::TableInfo {
        schema: None, name: "zoo_pg".into(), kind: dbclient_lib::drivers::TableKind::Table,
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
    assert_eq!(by_name("n"), &serde_json::json!("12345.6789")); // exact decimal as string
    assert_eq!(by_name("sm"), &serde_json::json!(7));
    assert_eq!(by_name("arr"), &serde_json::json!([1, 2, 3]));
    // int64 beyond 2^53 ships as a string to survive JSON.parse
    assert_eq!(by_name("big"), &serde_json::json!("9007199254740993"));
    d.execute_raw("DROP TABLE zoo_pg").await.unwrap();
}
