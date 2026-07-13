mod common;
use dbclient_lib::drivers::sqlite::SqliteDriver;
use dbclient_lib::drivers::SqlDriver;

#[tokio::test]
async fn sqlite_browse_battery() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap(); // test-support helper on the driver
    }
    common::assert_browse_battery(&d).await;
}

#[tokio::test]
async fn sqlite_query_battery() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_query_battery(&d).await;
}

#[tokio::test]
async fn sqlite_edit_battery() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_edit_battery(&d).await;
}

#[tokio::test]
async fn sqlite_structure_battery() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    common::assert_structure_battery(&d).await;
}

#[tokio::test]
async fn sqlite_txn_battery() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    for sql in common::SEED_SQL {
        d.execute_raw(sql).await.unwrap();
    }
    common::assert_txn_battery(&d).await;
}

/// regression (db-app-explorer round 2 B3): big int64 ships as string; real NULL stays NULL
#[tokio::test]
async fn sqlite_bigint_and_null() {
    let d = SqliteDriver::connect(":memory:").await.unwrap();
    d.execute_raw("CREATE TABLE b (id INTEGER PRIMARY KEY, v INTEGER)").await.unwrap();
    d.execute_raw("INSERT INTO b VALUES (1, 9007199254740993), (2, 42), (3, NULL)").await.unwrap();
    let t = dbclient_lib::drivers::TableInfo {
        schema: None, name: "b".into(), kind: dbclient_lib::drivers::TableKind::Table,
    };
    let page = d.fetch_rows(&t, &dbclient_lib::drivers::FetchOptions {
        limit: 10, offset: 0, sort: None, filters: vec![],
    }).await.unwrap();
    assert_eq!(page.rows[0][1], serde_json::json!("9007199254740993"));
    assert_eq!(page.rows[1][1], serde_json::json!(42));
    assert!(page.rows[2][1].is_null(), "real NULL must stay NULL");
}

#[tokio::test]
async fn sqlite_export_import_roundtrip() {
    use dbclient_lib::drivers::TableInfo;
    use dbclient_lib::impexp;
    let dir = std::env::temp_dir();
    let csv_path = dir.join("dbclient_exp.csv");
    let json_path = dir.join("dbclient_exp.json");

    let d = SqliteDriver::connect(":memory:").await.unwrap();
    for sql in common::SEED_SQL { d.execute_raw(sql).await.unwrap(); }
    let people = TableInfo { schema: None, name: "people".into(), kind: dbclient_lib::drivers::TableKind::Table };

    // export both formats
    let n = impexp::export_csv(&d, &people, csv_path.to_str().unwrap()).await.unwrap();
    assert_eq!(n, 3);
    let n = impexp::export_json(&d, &people, json_path.to_str().unwrap()).await.unwrap();
    assert_eq!(n, 3);

    // import into a fresh identical table (CSV)
    d.execute_raw("CREATE TABLE people2 (id INTEGER PRIMARY KEY, name VARCHAR(64), email VARCHAR(128), age INTEGER)").await.unwrap();
    let t2 = TableInfo { schema: None, name: "people2".into(), kind: dbclient_lib::drivers::TableKind::Table };
    let r = impexp::import_csv(&d, &t2, csv_path.to_str().unwrap()).await.unwrap();
    assert_eq!(r.inserted, 3);
    let page = d.fetch_rows(&t2, &dbclient_lib::drivers::FetchOptions { limit: 100, offset: 0, sort: None, filters: vec![] }).await.unwrap();
    assert_eq!(page.rows.len(), 3);

    // import JSON into another fresh table
    d.execute_raw("CREATE TABLE people3 (id INTEGER PRIMARY KEY, name VARCHAR(64), email VARCHAR(128), age INTEGER)").await.unwrap();
    let t3 = TableInfo { schema: None, name: "people3".into(), kind: dbclient_lib::drivers::TableKind::Table };
    let r = impexp::import_json(&d, &t3, json_path.to_str().unwrap()).await.unwrap();
    assert_eq!(r.inserted, 3);

    // import SQL dump
    let sql_path = dir.join("dbclient_dump.sql");
    std::fs::write(&sql_path, "CREATE TABLE dumped (a INTEGER); INSERT INTO dumped VALUES (1); INSERT INTO dumped VALUES (2);").unwrap();
    let r = impexp::import_sql(&d, sql_path.to_str().unwrap()).await.unwrap();
    assert_eq!(r.inserted, 3); // 3 statements

    let _ = std::fs::remove_file(&csv_path);
    let _ = std::fs::remove_file(&json_path);
    let _ = std::fs::remove_file(&sql_path);
}
