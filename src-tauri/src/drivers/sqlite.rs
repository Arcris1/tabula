use super::{build_select, AppliedResult, ColumnMeta, FetchOptions, QueryEvent, QuerySession, RowsPage, SqlDriver, TableColumns, TableInfo, TableKind, STREAM_CHUNK};
use futures_util::TryStreamExt;
use crate::error::AppError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Row, SqlitePool};
use std::str::FromStr;

pub struct SqliteDriver { pool: SqlitePool }

fn quote(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
fn cast_text(quoted: &str) -> String { format!("CAST({quoted} AS TEXT)") }
fn qmark(_: usize) -> String { "?".into() }

impl SqliteDriver {
    pub async fn connect(path: &str) -> Result<Self, AppError> {
        // shared cache lets pooled connections see the same :memory: database
        let opts = SqliteConnectOptions::from_str(&format!("sqlite:{path}"))
            .map_err(|e| AppError::connection(e.to_string()))?
            .create_if_missing(path == ":memory:")
            .shared_cache(path == ":memory:");
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(8))
            .connect_with(opts)
            .await?;
        Ok(Self { pool })
    }

}

pub(crate) fn cell(row: &SqliteRow, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    use sqlx::ValueRef as _;
    if let Ok(raw) = row.try_get_raw(i) {
        if raw.is_null() { return serde_json::Value::Null; }
    }
    if let Ok(v) = row.try_get::<i64, _>(i) { return super::int64_value(v); }
    if let Ok(v) = row.try_get::<f64, _>(i) { return super::float_value(v); }
    if let Ok(v) = row.try_get::<String, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(i) {
        return base64::engine::general_purpose::STANDARD.encode(v).into();
    }
    serde_json::Value::String(format!("\u{27e8}{}\u{27e9}", row.column(i).type_info()))
}

#[async_trait::async_trait]
impl SqlDriver for SqliteDriver {
    async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: None,
            name: r.get::<String, _>(0),
            kind: if r.get::<String, _>(1) == "view" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn columns_meta(&self) -> Result<Vec<TableColumns>, AppError> {
        let mut out = vec![];
        for t in self.list_tables().await? {
            let rows = sqlx::query(&format!("PRAGMA table_info({})", quote(&t.name)))
                .fetch_all(&self.pool).await?;
            out.push(TableColumns {
                schema: None,
                table: t.name.clone(),
                columns: rows.iter().map(|r| r.get::<String, _>(1)).collect(),
            });
        }
        Ok(out)
    }

    async fn acquire_session(&self) -> Result<Box<dyn QuerySession>, AppError> {
        Ok(Box::new(SqliteSession { conn: self.pool.acquire().await? }))
    }

    async fn kill_session(&self, _session_id: &str) -> Result<(), AppError> {
        Ok(()) // sqlite: cancel = task abort only
    }


    async fn primary_key(&self, table: &TableInfo) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(&format!("PRAGMA table_info({})", quote(&table.name)))
            .fetch_all(&self.pool).await?;
        let mut cols: Vec<(i64, String)> = rows.iter()
            .filter(|r| r.get::<i64, _>(5) > 0)
            .map(|r| (r.get::<i64, _>(5), r.get::<String, _>(1)))
            .collect();
        cols.sort_by_key(|(n, _)| *n);
        Ok(cols.into_iter().map(|(_, name)| name).collect())
    }

    fn preview_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Vec<String> {
        crate::changeset::render_changeset(table, cs, quote, crate::changeset::escape_std)
            .into_iter().map(|s| s.sql).collect()
    }

    async fn apply_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Result<AppliedResult, AppError> {
        let stmts = crate::changeset::render_changeset(table, cs, quote, crate::changeset::escape_std);
        let mut tx = self.pool.begin().await?;
        let mut res = AppliedResult::default();
        for s in &stmts {
            let r = sqlx::query(&s.sql).execute(&mut *tx).await?;
            if s.expect_one && r.rows_affected() != 1 {
                // dropping tx rolls the transaction back
                return Err(AppError::conflict(format!("row changed externally ({})", s.pk_desc))
                    .with_detail(s.sql.clone()));
            }
            match s.kind {
                crate::changeset::StatementKind::Update => res.updated += 1,
                crate::changeset::StatementKind::Insert => res.inserted += r.rows_affected(),
                crate::changeset::StatementKind::Delete => res.deleted += 1,
            }
        }
        tx.commit().await?;
        Ok(res)
    }

    async fn execute_raw(&self, sql: &str) -> Result<(), AppError> {
        sqlx::query(sql).execute(&self.pool).await?;
        Ok(())
    }

    fn ddl_flavor(&self) -> crate::ddl::Flavor { crate::ddl::Flavor::Sqlite }

    async fn list_functions(&self) -> Result<Vec<String>, AppError> {
        Ok(vec![]) // sqlite has no user-defined stored functions
    }

    async fn function_definition(&self, _name: &str) -> Result<String, AppError> {
        Ok(String::new())
    }

    async fn table_structure(&self, table: &TableInfo) -> Result<super::TableStructure, AppError> {
        // foreign keys: column name -> "referenced_table(referenced_col)"
        let fk_rows = sqlx::query(&format!("PRAGMA foreign_key_list({})", quote(&table.name)))
            .fetch_all(&self.pool).await?;
        let fks: std::collections::HashMap<String, String> = fk_rows.iter()
            .map(|r| (r.get::<String, _>(3), format!("{}({})", r.get::<String, _>(2), r.get::<String, _>(4))))
            .collect();

        let cols = sqlx::query(&format!("PRAGMA table_info({})", quote(&table.name)))
            .fetch_all(&self.pool).await?;
        let columns = cols.iter().map(|r| {
            let name: String = r.get(1);
            super::StructColumn {
                foreign_key: fks.get(&name).cloned(),
                comment: None, // sqlite has no column comments
                data_type: r.get::<String, _>(2),
                nullable: r.get::<i64, _>(3) == 0,
                default: r.try_get::<Option<String>, _>(4).unwrap_or(None),
                is_pk: r.get::<i64, _>(5) > 0,
                is_computed: false, // detected via table_xinfo; omitted for simplicity
                name,
            }
        }).collect();

        let mut indexes = vec![];
        let idx_rows = sqlx::query(&format!("PRAGMA index_list({})", quote(&table.name)))
            .fetch_all(&self.pool).await?;
        for r in &idx_rows {
            let origin: String = r.get(3);
            if origin == "pk" { continue; }
            let name: String = r.get(1);
            let unique: i64 = r.get(2);
            let members = sqlx::query(&format!("PRAGMA index_info({})", quote(&name)))
                .fetch_all(&self.pool).await?;
            indexes.push(super::StructIndex {
                name: name.clone(),
                columns: members.iter().map(|m| m.get::<String, _>(2)).collect(),
                unique: unique == 1,
            });
        }
        let trig_rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ? ORDER BY name",
        ).bind(&table.name).fetch_all(&self.pool).await?;
        let triggers = trig_rows.iter().map(|r| super::StructTrigger {
            name: r.get::<String, _>(0), timing: String::new(), event: String::new(),
        }).collect();

        Ok(super::TableStructure { columns, indexes, triggers, comment: None })
    }
    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let pk = self.primary_key(table).await.unwrap_or_default();
        let (sql, binds) = build_select(table, opts, &pk, quote, cast_text, qmark, crate::changeset::escape_std);
        let mut q = sqlx::query(&sql);
        for b in &binds { q = q.bind(b); }
        let rows = q.fetch_all(&self.pool).await?;

        let columns: Vec<ColumnMeta> = match rows.first() {
            Some(r) => r.columns().iter()
                .map(|c| ColumnMeta { name: c.name().to_string(), data_type: format!("{:?}", c.type_info()) })
                .collect(),
            // empty result: describe the table so the grid still has headers
            None => sqlx::query(&format!("PRAGMA table_info({})", quote(&table.name)))
                .fetch_all(&self.pool).await?
                .iter()
                .map(|r| ColumnMeta { name: r.get::<String, _>(1), data_type: r.get::<String, _>(2) })
                .collect(),
        };

        let count_sql = format!("SELECT COUNT(*) FROM {}", quote(&table.name));
        let total: i64 = sqlx::query_scalar(&count_sql).fetch_one(&self.pool).await?;

        Ok(RowsPage {
            query: sql,
            columns,
            rows: rows.iter().map(|r| (0..r.columns().len()).map(|i| cell(r, i)).collect()).collect(),
            total_estimate: Some(total),
        })
    }
}

pub struct SqliteSession { conn: sqlx::pool::PoolConnection<sqlx::Sqlite> }

fn row_cells(row: &SqliteRow) -> Vec<serde_json::Value> {
    (0..row.columns().len()).map(|i| cell(row, i)).collect()
}
fn row_columns(row: &SqliteRow) -> Vec<ColumnMeta> {
    row.columns().iter()
        .map(|c| ColumnMeta { name: c.name().to_string(), data_type: format!("{:?}", c.type_info()) })
        .collect()
}

#[async_trait::async_trait]
impl QuerySession for SqliteSession {
    fn session_id(&self) -> Option<String> { None }

    async fn stream(&mut self, sql: &str, tx: tokio::sync::mpsc::Sender<QueryEvent>) -> Result<(), AppError> {
        if super::is_unpreparable(sql) {
            use sqlx::Executor;
            // plain &str executes over the simple/text protocol (unprepared)
            let res = (&mut *self.conn).execute(sql).await.map_err(AppError::from)?;
            let _ = tx.send(QueryEvent::Done { row_count: 0, affected_rows: res.rows_affected() }).await;
            return Ok(());
        }
        let mut stream = sqlx::query(sql).fetch_many(&mut *self.conn);
        // `Left` ends a result set; after rows, reset so the next set is its own grid.
        let mut sent_cols = false;
        let mut chunk: Vec<Vec<serde_json::Value>> = vec![];
        let (mut row_count, mut affected) = (0u64, 0u64);
        while let Some(item) = stream.try_next().await.map_err(AppError::from)? {
            match item {
                sqlx::Either::Left(res) => {
                    affected += res.rows_affected();
                    if sent_cols {
                        if !chunk.is_empty() { let _ = tx.send(QueryEvent::Rows(std::mem::take(&mut chunk))).await; }
                        sent_cols = false;
                    }
                }
                sqlx::Either::Right(row) => {
                    if !sent_cols {
                        if !chunk.is_empty() { let _ = tx.send(QueryEvent::Rows(std::mem::take(&mut chunk))).await; }
                        let _ = tx.send(QueryEvent::Columns(row_columns(&row))).await;
                        sent_cols = true;
                    }
                    chunk.push(row_cells(&row));
                    row_count += 1;
                    if chunk.len() >= STREAM_CHUNK {
                        let _ = tx.send(QueryEvent::Rows(std::mem::take(&mut chunk))).await;
                    }
                }
            }
        }
        if !chunk.is_empty() { let _ = tx.send(QueryEvent::Rows(chunk)).await; }
        let _ = tx.send(QueryEvent::Done { row_count, affected_rows: affected }).await;
        Ok(())
    }
}
