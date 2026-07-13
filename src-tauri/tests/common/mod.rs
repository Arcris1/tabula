use dbclient_lib::drivers::{FetchOptions, Filter, FilterOp, Sort, SqlDriver};
#[allow(unused_imports)]
use dbclient_lib::drivers::QuerySession;

pub const SEED_SQL: &[&str] = &[
    "DROP TABLE IF EXISTS people",
    "CREATE TABLE people (id INTEGER PRIMARY KEY, name VARCHAR(64), email VARCHAR(128), age INTEGER)",
    "INSERT INTO people (id, name, email, age) VALUES (1, 'Alice', 'alice@gmail.com', 30)",
    "INSERT INTO people (id, name, email, age) VALUES (2, 'Bob', 'bob@yahoo.com', 25)",
    "INSERT INTO people (id, name, email, age) VALUES (3, 'Carol', 'carol@gmail.com', 41)",
];

fn no_opts() -> FetchOptions {
    FetchOptions { limit: 100, offset: 0, sort: None, filters: vec![] }
}

pub async fn assert_browse_battery(d: &dyn SqlDriver) {
    d.ping().await.expect("ping");

    let tables = d.list_tables().await.expect("list_tables");
    let people = tables.iter().find(|t| t.name == "people").expect("people table listed");

    // full read
    let page = d.fetch_rows(people, &no_opts()).await.expect("fetch");
    assert_eq!(page.rows.len(), 3);
    let name_idx = page.columns.iter().position(|c| c.name == "name").expect("name col");

    // sort desc by id -> Carol first
    let sorted = d.fetch_rows(people, &FetchOptions {
        sort: Some(Sort { column: "id".into(), desc: true }), ..no_opts()
    }).await.expect("sorted fetch");
    assert_eq!(sorted.rows[0][name_idx], serde_json::json!("Carol"));

    // filter contains
    let filtered = d.fetch_rows(people, &FetchOptions {
        filters: vec![Filter { column: "email".into(), op: FilterOp::Contains, value: "gmail".into() }],
        ..no_opts()
    }).await.expect("filtered fetch");
    assert_eq!(filtered.rows.len(), 2);

    // pagination
    let page2 = d.fetch_rows(people, &FetchOptions {
        limit: 2, offset: 2,
        sort: Some(Sort { column: "id".into(), desc: false }), ..no_opts()
    }).await.expect("paged fetch");
    assert_eq!(page2.rows.len(), 1);

    // errors are typed
    let missing = dbclient_lib::drivers::TableInfo {
        schema: None, name: "no_such_table".into(),
        kind: dbclient_lib::drivers::TableKind::Table,
    };
    let err = d.fetch_rows(&missing, &no_opts()).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::QueryError);
}

pub async fn assert_query_battery(d: &dyn SqlDriver) {
    // columns_meta lists people(id,name,email,age)
    let meta = d.columns_meta().await.expect("columns_meta");
    let people = meta.iter().find(|t| t.table == "people").expect("people in meta");
    assert_eq!(people.columns, vec!["id", "name", "email", "age"]);

    // stream a select: Columns event, Rows chunk, Done with row_count 3
    let mut session = d.acquire_session().await.expect("session");
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    session.stream("SELECT * FROM people ORDER BY id", tx).await.expect("stream");
    let mut got_cols = false;
    let mut rows = 0u64;
    let mut done = None;
    while let Some(ev) = rx.recv().await {
        match ev {
            dbclient_lib::drivers::QueryEvent::Columns(c) => { assert_eq!(c.len(), 4); got_cols = true; }
            dbclient_lib::drivers::QueryEvent::Rows(r) => rows += r.len() as u64,
            dbclient_lib::drivers::QueryEvent::Done { row_count, .. } => done = Some(row_count),
        }
    }
    assert!(got_cols);
    assert_eq!(rows, 3);
    assert_eq!(done, Some(3));
    drop(session); // return the pooled connection before acquiring another

    // errors are typed
    let mut bad = d.acquire_session().await.expect("session2");
    let (tx2, _rx2) = tokio::sync::mpsc::channel(16);
    let err = bad.stream("SELECT * FROM no_such_table", tx2).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::QueryError);
}

use dbclient_lib::changeset::{CellChange, Changeset, RowDelete, RowUpdate};

fn pk_map(v: i64) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("id".into(), serde_json::json!(v));
    m
}

pub async fn assert_edit_battery(d: &dyn SqlDriver) {
    let tables = d.list_tables().await.unwrap();
    let people = tables.iter().find(|t| t.name == "people").unwrap();

    // pk detection
    assert_eq!(d.primary_key(people).await.unwrap(), vec!["id"]);

    // update + insert + delete in one changeset
    let mut ins = serde_json::Map::new();
    ins.insert("id".into(), serde_json::json!(4));
    ins.insert("name".into(), serde_json::json!("Dave"));
    ins.insert("email".into(), serde_json::json!("dave@x.com"));
    ins.insert("age".into(), serde_json::json!(22));
    let cs = Changeset {
        updates: vec![RowUpdate { pk: pk_map(1), changes: vec![
            CellChange { column: "age".into(), old: serde_json::json!(30), new: serde_json::json!(31) },
        ]}],
        deletes: vec![RowDelete { pk: pk_map(2) }],
        inserts: vec![ins],
    };
    // preview shows 3 statements, commit applies them
    assert_eq!(d.preview_changeset(people, &cs).len(), 3);
    let res = d.apply_changeset(people, &cs).await.unwrap();
    assert_eq!((res.updated, res.deleted, res.inserted), (1, 1, 1));

    let page = d.fetch_rows(people, &FetchOptions { limit: 100, offset: 0, sort: None, filters: vec![] }).await.unwrap();
    assert_eq!(page.rows.len(), 3); // 3 - 1 deleted + 1 inserted

    // conflict: stale old value -> ConflictOnCommit, transaction rolled back
    let stale = Changeset {
        updates: vec![RowUpdate { pk: pk_map(1), changes: vec![
            CellChange { column: "age".into(), old: serde_json::json!(30) /* actually 31 now */, new: serde_json::json!(99) },
        ]}],
        deletes: vec![RowDelete { pk: pk_map(3) }], // would succeed, must be rolled back
        inserts: vec![],
    };
    let err = d.apply_changeset(people, &stale).await.unwrap_err();
    assert_eq!(err.kind, dbclient_lib::error::ErrorKind::ConflictOnCommit);
    let after = d.fetch_rows(people, &FetchOptions { limit: 100, offset: 0, sort: None, filters: vec![] }).await.unwrap();
    assert_eq!(after.rows.len(), 3, "delete must have been rolled back");
}

use dbclient_lib::ddl::{ColumnDef, DdlOp};

fn mk_table(name: &str) -> dbclient_lib::drivers::TableInfo {
    dbclient_lib::drivers::TableInfo {
        schema: None, name: name.into(), kind: dbclient_lib::drivers::TableKind::Table,
    }
}

pub async fn assert_structure_battery(d: &dyn SqlDriver) {
    let flavor = d.ddl_flavor();
    // clean slate (ignore errors: objects may not exist)
    let _ = d.execute_raw(&dbclient_lib::ddl::render_ddl(
        &DdlOp::DropIndex { table: mk_table("m4_t"), name: "idx_m4".into() }, flavor)).await;
    let _ = d.execute_raw("DROP TABLE IF EXISTS m4_t").await;

    d.execute_raw(&dbclient_lib::ddl::render_ddl(&DdlOp::CreateTable {
        name: "m4_t".into(),
        columns: vec![
            ColumnDef { name: "id".into(), data_type: "INTEGER".into(), nullable: false, is_pk: true },
            ColumnDef { name: "label".into(), data_type: "VARCHAR(32)".into(), nullable: true, is_pk: false },
        ],
    }, flavor)).await.expect("create table");

    d.execute_raw(&dbclient_lib::ddl::render_ddl(&DdlOp::AddColumn {
        table: mk_table("m4_t"),
        column: ColumnDef { name: "qty".into(), data_type: "INTEGER".into(), nullable: true, is_pk: false },
    }, flavor)).await.expect("add column");

    d.execute_raw(&dbclient_lib::ddl::render_ddl(&DdlOp::AddIndex {
        table: mk_table("m4_t"), name: "idx_m4".into(), columns: vec!["label".into()], unique: false,
    }, flavor)).await.expect("add index");

    // regression (db-app-explorer B5): an EMPTY table still reports its columns
    let empty_page = d.fetch_rows(&mk_table("m4_t"), &FetchOptions {
        limit: 10, offset: 0, sort: None, filters: vec![],
    }).await.expect("fetch empty");
    assert_eq!(empty_page.rows.len(), 0);
    assert_eq!(empty_page.columns.len(), 3, "empty result must still describe columns");

    let st = d.table_structure(&mk_table("m4_t")).await.expect("structure");
    assert_eq!(st.columns.len(), 3);
    let id = st.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id.is_pk);
    let qty = st.columns.iter().find(|c| c.name == "qty").unwrap();
    assert!(qty.nullable);
    assert!(st.indexes.iter().any(|i| i.name == "idx_m4" && i.columns == vec!["label"] && !i.unique));
    // triggers/comment fields exist (empty on this fixture) — struct shape is what we assert here
    let _ = &st.triggers;
    let _ = &st.comment;
    assert!(st.columns.iter().all(|c| c.foreign_key.is_none()));
}

/// drains a session stream fully, returning Err if the statement failed
async fn exec_on(session: &mut Box<dyn dbclient_lib::drivers::QuerySession>, sql: &str) -> Result<(), dbclient_lib::error::AppError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let res = session.stream(sql, tx).await;
    while rx.recv().await.is_some() {}
    res
}

fn people() -> dbclient_lib::drivers::TableInfo {
    mk_table("people")
}

async fn count_people(d: &dyn SqlDriver) -> usize {
    d.fetch_rows(&people(), &FetchOptions { limit: 100, offset: 0, sort: None, filters: vec![] })
        .await.unwrap().rows.len()
}

/// One session reused across statements => real transactions. Caller re-seeds first.
pub async fn assert_txn_battery(d: &dyn SqlDriver) {
    assert_eq!(count_people(d).await, 3, "battery expects a fresh seed");

    // ROLLBACK restores the deleted row
    let mut s = d.acquire_session().await.unwrap();
    exec_on(&mut s, "BEGIN").await.unwrap();
    exec_on(&mut s, "DELETE FROM people WHERE id = 1").await.unwrap();
    exec_on(&mut s, "ROLLBACK").await.unwrap();
    drop(s);
    assert_eq!(count_people(d).await, 3, "ROLLBACK must undo the delete");

    // COMMIT persists it
    let mut s = d.acquire_session().await.unwrap();
    exec_on(&mut s, "BEGIN").await.unwrap();
    exec_on(&mut s, "DELETE FROM people WHERE id = 1").await.unwrap();
    exec_on(&mut s, "COMMIT").await.unwrap();
    drop(s);
    assert_eq!(count_people(d).await, 2, "COMMIT must persist the delete");
}
