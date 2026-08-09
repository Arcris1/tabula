pub mod keytree;
pub mod mssql;
pub mod mysql;
pub mod postgres;
pub mod redis_kv;
pub mod sqlite;

use crate::error::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TableKind { Table, View }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub schema: Option<String>,
    pub name: String,
    pub kind: TableKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbInfo {
    pub name: String,
    pub size_bytes: Option<i64>,
}

/// Database names are interpolated raw into `CREATE`/`DROP DATABASE` (which
/// can't be parameterized), so restrict them to a safe identifier shape.
pub fn valid_db_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.len() <= 64
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedResult {
    pub updated: u64,
    pub inserted: u64,
    pub deleted: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsPage {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total_estimate: Option<i64>,
    /// the SELECT the app ran for this page (shown in the SQL panel)
    pub query: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp { Contains, Equals, IsNull, NotNull }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub column: String,
    pub op: FilterOp,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub column: String,
    pub desc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchOptions {
    pub limit: i64,
    pub offset: i64,
    pub sort: Option<Sort>,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyScanPage {
    pub keys: Vec<String>,
    pub cursor: u64,
}

#[derive(Debug, Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RedisValueDto {
    String { value: String },
    Hash { fields: Vec<(String, String)> },
    List { items: Vec<String> },
    Set { members: Vec<String> },
    ZSet { members: Vec<(String, f64)> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisEntry {
    pub key: String,
    /// seconds; -1 = no TTL, -2 = key missing
    pub ttl_secs: i64,
    pub value: RedisValueDto,
    /// full server-side length for capped types (list/zset); None = value is complete
    pub total_items: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_pk: bool,
    pub is_computed: bool,
    /// "other_table(col)" when this column references another table
    pub foreign_key: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructTrigger {
    pub name: String,
    pub timing: String,
    pub event: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    pub columns: Vec<StructColumn>,
    pub indexes: Vec<StructIndex>,
    pub triggers: Vec<StructTrigger>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumns {
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Debug)]
pub enum QueryEvent {
    Columns(Vec<ColumnMeta>),
    Rows(Vec<Vec<serde_json::Value>>),
    /// server info message (T-SQL PRINT / RAISERROR severity<11, like SSMS's Messages tab)
    Message(String),
    Done { row_count: u64, affected_rows: u64 },
}

pub const STREAM_CHUNK: usize = 200;

/// JS Number.MAX_SAFE_INTEGER: larger int64s corrupt in JSON.parse, so ship as strings
pub(crate) const MAX_SAFE_INT: i64 = 9_007_199_254_740_992;

pub(crate) fn int64_value(v: i64) -> serde_json::Value {
    if v.abs() > MAX_SAFE_INT { v.to_string().into() } else { v.into() }
}

pub(crate) fn uint64_value(v: u64) -> serde_json::Value {
    if v > MAX_SAFE_INT as u64 { v.to_string().into() } else { v.into() }
}

pub(crate) fn float_value(v: f64) -> serde_json::Value {
    if v.is_finite() {
        serde_json::Number::from_f64(v).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null)
    } else {
        v.to_string().into() // NaN / inf are not representable in JSON numbers
    }
}

/// Statements that cannot go through the prepared-statement protocol
/// (MySQL rejects them; run via the unprepared raw path instead).
pub fn is_unpreparable(sql: &str) -> bool {
    let first = sql.trim_start().split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    matches!(first.as_str(),
        "begin" | "commit" | "rollback" | "start" | "savepoint" | "release"
        | "set" | "use" | "lock" | "unlock")
}

#[async_trait::async_trait]
pub trait QuerySession: Send {
    fn session_id(&self) -> Option<String>;
    async fn stream(&mut self, sql: &str, tx: tokio::sync::mpsc::Sender<QueryEvent>) -> Result<(), AppError>;
}

#[async_trait::async_trait]
pub trait SqlDriver: Send + Sync {
    async fn ping(&self) -> Result<(), AppError>;
    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError>;
    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError>;
    async fn columns_meta(&self) -> Result<Vec<TableColumns>, AppError>;
    async fn acquire_session(&self) -> Result<Box<dyn QuerySession>, AppError>;
    async fn kill_session(&self, session_id: &str) -> Result<(), AppError>;
    async fn primary_key(&self, table: &TableInfo) -> Result<Vec<String>, AppError>;
    fn preview_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Vec<String>;
    async fn apply_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Result<AppliedResult, AppError>;
    async fn execute_raw(&self, sql: &str) -> Result<(), AppError>;
    async fn table_structure(&self, table: &TableInfo) -> Result<TableStructure, AppError>;
    fn ddl_flavor(&self) -> crate::ddl::Flavor;
    /// user-defined functions/procedures (empty for engines without them)
    async fn list_functions(&self) -> Result<Vec<String>, AppError>;
    /// source (CREATE FUNCTION/PROCEDURE ...) of a named function; empty if none
    async fn function_definition(&self, name: &str) -> Result<String, AppError>;

    /// databases/schemas on this server (empty for single-file engines)
    async fn list_databases(&self) -> Result<Vec<DbInfo>, AppError> { Ok(vec![]) }
    async fn create_database(&self, _name: &str) -> Result<(), AppError> {
        Err(AppError::query("this engine does not support creating databases"))
    }
    async fn drop_database(&self, _name: &str) -> Result<(), AppError> {
        Err(AppError::query("this engine does not support dropping databases"))
    }
}

#[async_trait::async_trait]
pub trait KvDriver: Send + Sync {
    async fn ping(&self) -> Result<(), AppError>;
    async fn scan_keys(&self, pattern: &str, cursor: u64, count: u32) -> Result<KeyScanPage, AppError>;
    async fn get_value(&self, key: &str) -> Result<RedisEntry, AppError>;
    /// DEL + typed rewrite in a MULTI pipeline; preserves a positive TTL
    async fn set_value(&self, key: &str, value: &RedisValueDto) -> Result<(), AppError>;
    async fn delete_key(&self, key: &str) -> Result<(), AppError>;
    /// ttl_secs <= 0 => PERSIST
    async fn set_ttl(&self, key: &str, ttl_secs: i64) -> Result<(), AppError>;
    /// Switch the active logical database (SELECT n) on this connection.
    async fn select_db(&self, index: i64) -> Result<(), AppError>;
}

pub fn qualified_name(table: &TableInfo, quote: fn(&str) -> String) -> String {
    match &table.schema {
        Some(s) => format!("{}.{}", quote(s), quote(&table.name)),
        None => quote(&table.name),
    }
}

/// Escape LIKE wildcards in a user-typed Contains value; paired with
/// `ESCAPE '!'` in the SQL so `%`/`_` in the value match literally.
pub(crate) fn escape_like(v: &str) -> String {
    v.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

pub fn build_select(
    table: &TableInfo,
    opts: &FetchOptions,
    pk: &[String],
    quote: fn(&str) -> String,
    cast_text: fn(&str) -> String,
    placeholder: fn(usize) -> String,
    escape: fn(&str) -> String,
) -> (String, Vec<String>) {
    let qualified = qualified_name(table, quote);
    let mut sql = format!("SELECT * FROM {qualified}");
    let mut binds: Vec<String> = vec![];
    let mut clauses: Vec<String> = vec![];
    for f in &opts.filters {
        let col = quote(&f.column);
        match f.op {
            FilterOp::Contains => {
                binds.push(format!("%{}%", escape_like(&f.value)));
                clauses.push(format!("{} LIKE {} ESCAPE '!'", cast_text(&col), placeholder(binds.len())));
            }
            FilterOp::Equals => {
                // Inline the value as an escaped literal (the same escaping the
                // changeset write path uses) instead of CAST-ing the COLUMN to
                // text: the engine coerces the literal to the column's type, so
                // an indexed `id = '123'` stays an index lookup instead of a
                // full-table scan.
                clauses.push(format!("{} = '{}'", col, escape(&f.value)));
            }
            FilterOp::IsNull => clauses.push(format!("{col} IS NULL")),
            FilterOp::NotNull => clauses.push(format!("{col} IS NOT NULL")),
        }
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    // Deterministic paging: LIMIT/OFFSET without a total order can skip or
    // duplicate rows between pages. Order by the user's sort with the PK as
    // tiebreaker, or by the PK alone; tables without a PK keep old behavior.
    let mut order: Vec<String> = vec![];
    if let Some(sort) = &opts.sort {
        order.push(format!("{} {}", quote(&sort.column), if sort.desc { "DESC" } else { "ASC" }));
        order.extend(pk.iter().filter(|k| **k != sort.column).map(|k| quote(k)));
    } else {
        order.extend(pk.iter().map(|k| quote(k)));
    }
    if !order.is_empty() {
        sql.push_str(&format!(" ORDER BY {}", order.join(", ")));
    }
    sql.push_str(&format!(" LIMIT {} OFFSET {}", opts.limit, opts.offset));
    (sql, binds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> String { format!("\"{}\"", s) }
    fn c(s: &str) -> String { format!("CAST({s} AS TEXT)") }
    fn dollar(i: usize) -> String { format!("${i}") }
    fn qmark(_: usize) -> String { "?".into() }

    fn t() -> TableInfo {
        TableInfo { schema: Some("public".into()), name: "users".into(), kind: TableKind::Table }
    }
    fn esc(s: &str) -> String { s.replace('\'', "''") }
    const PK: &[String] = &[];

    #[test]
    fn plain_select_orders_by_pk() {
        let opts = FetchOptions { limit: 500, offset: 1000, sort: None, filters: vec![] };
        let pk = vec!["id".to_string()];
        let (sql, binds) = build_select(&t(), &opts, &pk, q, c, dollar, esc);
        assert_eq!(sql, r#"SELECT * FROM "public"."users" ORDER BY "id" LIMIT 500 OFFSET 1000"#);
        assert!(binds.is_empty());
    }

    #[test]
    fn no_pk_keeps_unordered_paging() {
        let opts = FetchOptions { limit: 500, offset: 0, sort: None, filters: vec![] };
        let (sql, _) = build_select(&t(), &opts, PK, q, c, dollar, esc);
        assert_eq!(sql, r#"SELECT * FROM "public"."users" LIMIT 500 OFFSET 0"#);
    }

    #[test]
    fn sort_gets_pk_tiebreaker_but_not_when_sorting_by_pk() {
        let pk = vec!["id".to_string()];
        let by_status = FetchOptions {
            limit: 10, offset: 0,
            sort: Some(Sort { column: "status".into(), desc: false }), filters: vec![],
        };
        let (sql, _) = build_select(&t(), &by_status, &pk, q, c, dollar, esc);
        assert!(sql.contains(r#"ORDER BY "status" ASC, "id" LIMIT"#), "{sql}");

        let by_id = FetchOptions {
            limit: 10, offset: 0,
            sort: Some(Sort { column: "id".into(), desc: true }), filters: vec![],
        };
        let (sql, _) = build_select(&t(), &by_id, &pk, q, c, dollar, esc);
        assert!(sql.contains(r#"ORDER BY "id" DESC LIMIT"#), "{sql}");
    }

    #[test]
    fn filters_sort_and_placeholders() {
        let pk = vec!["id".to_string()];
        let opts = FetchOptions {
            limit: 500, offset: 0,
            sort: Some(Sort { column: "id".into(), desc: true }),
            filters: vec![
                Filter { column: "email".into(), op: FilterOp::Contains, value: "gmail".into() },
                Filter { column: "name".into(), op: FilterOp::Equals, value: "bob".into() },
                Filter { column: "deleted_at".into(), op: FilterOp::IsNull, value: String::new() },
            ],
        };
        let (sql, binds) = build_select(&t(), &opts, &pk, q, c, dollar, esc);
        assert_eq!(
            sql,
            r#"SELECT * FROM "public"."users" WHERE CAST("email" AS TEXT) LIKE $1 ESCAPE '!' AND "name" = 'bob' AND "deleted_at" IS NULL ORDER BY "id" DESC LIMIT 500 OFFSET 0"#
        );
        // equals is an inline literal now — only the LIKE pattern binds
        assert_eq!(binds, vec!["%gmail%".to_string()]);
    }

    #[test]
    fn equals_is_native_and_escaped() {
        let opts = FetchOptions {
            limit: 10, offset: 0, sort: None,
            filters: vec![Filter { column: "a".into(), op: FilterOp::Equals, value: "x' OR '1'='1".into() }],
        };
        let (sql, binds) = build_select(&t(), &opts, PK, q, c, qmark, esc);
        // no CAST on the column (sargable) and quotes doubled (no injection)
        assert!(sql.contains(r#""a" = 'x'' OR ''1''=''1'"#), "{sql}");
        assert!(binds.is_empty());
    }

    #[test]
    fn contains_escapes_like_wildcards() {
        let opts = FetchOptions {
            limit: 10, offset: 0, sort: None,
            filters: vec![Filter { column: "phone".into(), op: FilterOp::Contains, value: "50%_!".into() }],
        };
        let (sql, binds) = build_select(&t(), &opts, PK, q, c, qmark, esc);
        assert!(sql.contains(r#"LIKE ? ESCAPE '!'"#), "{sql}");
        assert_eq!(binds, vec!["%50!%!_!!%".to_string()]);
    }
}
