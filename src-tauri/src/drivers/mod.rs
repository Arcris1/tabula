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
}

pub fn qualified_name(table: &TableInfo, quote: fn(&str) -> String) -> String {
    match &table.schema {
        Some(s) => format!("{}.{}", quote(s), quote(&table.name)),
        None => quote(&table.name),
    }
}

pub fn build_select(
    table: &TableInfo,
    opts: &FetchOptions,
    quote: fn(&str) -> String,
    cast_text: fn(&str) -> String,
    placeholder: fn(usize) -> String,
) -> (String, Vec<String>) {
    let qualified = qualified_name(table, quote);
    let mut sql = format!("SELECT * FROM {qualified}");
    let mut binds: Vec<String> = vec![];
    let mut clauses: Vec<String> = vec![];
    for f in &opts.filters {
        let col = quote(&f.column);
        match f.op {
            FilterOp::Contains => {
                binds.push(format!("%{}%", f.value));
                clauses.push(format!("{} LIKE {}", cast_text(&col), placeholder(binds.len())));
            }
            FilterOp::Equals => {
                binds.push(f.value.clone());
                clauses.push(format!("{} = {}", cast_text(&col), placeholder(binds.len())));
            }
            FilterOp::IsNull => clauses.push(format!("{col} IS NULL")),
            FilterOp::NotNull => clauses.push(format!("{col} IS NOT NULL")),
        }
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    if let Some(sort) = &opts.sort {
        sql.push_str(&format!(
            " ORDER BY {} {}",
            quote(&sort.column),
            if sort.desc { "DESC" } else { "ASC" }
        ));
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

    #[test]
    fn plain_select_with_paging() {
        let opts = FetchOptions { limit: 500, offset: 1000, sort: None, filters: vec![] };
        let (sql, binds) = build_select(&t(), &opts, q, c, dollar);
        assert_eq!(sql, r#"SELECT * FROM "public"."users" LIMIT 500 OFFSET 1000"#);
        assert!(binds.is_empty());
    }

    #[test]
    fn filters_sort_and_placeholders() {
        let opts = FetchOptions {
            limit: 500, offset: 0,
            sort: Some(Sort { column: "id".into(), desc: true }),
            filters: vec![
                Filter { column: "email".into(), op: FilterOp::Contains, value: "gmail".into() },
                Filter { column: "name".into(), op: FilterOp::Equals, value: "bob".into() },
                Filter { column: "deleted_at".into(), op: FilterOp::IsNull, value: String::new() },
            ],
        };
        let (sql, binds) = build_select(&t(), &opts, q, c, dollar);
        assert_eq!(
            sql,
            r#"SELECT * FROM "public"."users" WHERE CAST("email" AS TEXT) LIKE $1 AND CAST("name" AS TEXT) = $2 AND "deleted_at" IS NULL ORDER BY "id" DESC LIMIT 500 OFFSET 0"#
        );
        assert_eq!(binds, vec!["%gmail%".to_string(), "bob".to_string()]);
    }

    #[test]
    fn qmark_placeholders_and_no_schema() {
        let table = TableInfo { schema: None, name: "t".into(), kind: TableKind::Table };
        let opts = FetchOptions {
            limit: 10, offset: 0, sort: None,
            filters: vec![Filter { column: "a".into(), op: FilterOp::Equals, value: "1".into() }],
        };
        let (sql, _) = build_select(&table, &opts, q, c, qmark);
        assert_eq!(sql, r#"SELECT * FROM "t" WHERE CAST("a" AS TEXT) = ? LIMIT 10 OFFSET 0"#);
    }
}
