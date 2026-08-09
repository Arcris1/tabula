use super::{build_select, AppliedResult, ColumnMeta, FetchOptions, QueryEvent, QuerySession, RowsPage, SqlDriver, TableColumns, TableInfo, TableKind, STREAM_CHUNK};
use futures_util::TryStreamExt;
use crate::error::AppError;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow};
use sqlx::{Column, PgPool, Row};
use std::str::FromStr;

pub struct PostgresDriver { pool: PgPool }

fn quote(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
fn cast_text(quoted: &str) -> String { format!("CAST({quoted} AS TEXT)") }
fn dollar(i: usize) -> String { format!("${i}") }

impl PostgresDriver {
    pub async fn connect(host: &str, port: u16, user: &str, password: &str, db: &str) -> Result<Self, AppError> {
        let opts = PgConnectOptions::new()
            .host(host).port(port).username(user).password(password).database(db);
        Self::from_opts(opts).await
    }

    pub async fn connect_url(url: &str) -> Result<Self, AppError> {
        let opts = PgConnectOptions::from_str(url).map_err(|e| AppError::connection(e.to_string()))?;
        Self::from_opts(opts).await
    }

    async fn from_opts(opts: PgConnectOptions) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new().max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(8))
            // Reconnect transparently after server-side idle timeouts: ping stale
            // connections before handing them out, recycle idle/old ones proactively.
            .test_before_acquire(true)
            .idle_timeout(std::time::Duration::from_secs(600))
            .max_lifetime(std::time::Duration::from_secs(1800))
            .connect_with(opts).await?;
        Ok(Self { pool })
    }

}

pub(crate) fn cell(row: &PgRow, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    use sqlx::ValueRef as _;
    if let Ok(raw) = row.try_get_raw(i) {
        if raw.is_null() { return serde_json::Value::Null; }
    }
    if let Ok(v) = row.try_get::<i64, _>(i) { return super::int64_value(v); }
    if let Ok(v) = row.try_get::<i32, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<i16, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<f64, _>(i) { return super::float_value(v); }
    if let Ok(v) = row.try_get::<f32, _>(i) { return super::float_value(v as f64); }
    if let Ok(v) = row.try_get::<bool, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<String, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<sqlx::types::Decimal, _>(i) { return v.to_string().into(); } // exact, not lossy f64
    if let Ok(v) = row.try_get::<sqlx::types::Uuid, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) { return v.to_rfc3339().into(); }
    if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<sqlx::postgres::types::PgInterval, _>(i) {
        return format!("{} mon {} days {} us", v.months, v.days, v.microseconds).into();
    }
    if let Ok(v) = row.try_get::<sqlx::postgres::types::PgMoney, _>(i) {
        return format!("{:.2}", v.0 as f64 / 100.0).into();
    }
    if let Ok(v) = row.try_get::<serde_json::Value, _>(i) { return v; }
    if let Ok(v) = row.try_get::<Vec<i64>, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<Vec<i32>, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<Vec<f64>, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<Vec<String>, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(i) {
        return base64::engine::general_purpose::STANDARD.encode(v).into();
    }
    // visible marker: never render an undecodable value as a fake NULL
    serde_json::Value::String(format!("\u{27e8}{}\u{27e9}", row.column(i).type_info()))
}

#[async_trait::async_trait]
impl SqlDriver for PostgresDriver {
    async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT table_schema, table_name, table_type FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY table_schema, table_name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: Some(r.get::<String, _>(0)),
            name: r.get::<String, _>(1),
            kind: if r.get::<String, _>(2) == "VIEW" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn columns_meta(&self) -> Result<Vec<TableColumns>, AppError> {
        let rows = sqlx::query(
            "SELECT table_schema, table_name, column_name FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY table_schema, table_name, ordinal_position",
        ).fetch_all(&self.pool).await?;
        let mut out: Vec<TableColumns> = vec![];
        for r in rows {
            let (s, t, c): (String, String, String) = (r.get(0), r.get(1), r.get(2));
            let schema = if s == "public" { None } else { Some(s) };
            match out.last_mut() {
                Some(last) if last.table == t && last.schema == schema => last.columns.push(c),
                _ => out.push(TableColumns { schema, table: t, columns: vec![c] }),
            }
        }
        Ok(out)
    }

    async fn acquire_session(&self) -> Result<Box<dyn QuerySession>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *conn).await?;
        Ok(Box::new(PgSession { conn, pid }))
    }

    async fn kill_session(&self, session_id: &str) -> Result<(), AppError> {
        let pid: i32 = session_id.parse().map_err(|_| AppError::internal("bad session id"))?;
        sqlx::query("SELECT pg_cancel_backend($1)").bind(pid).execute(&self.pool).await?;
        Ok(())
    }


    async fn primary_key(&self, table: &TableInfo) -> Result<Vec<String>, AppError> {
        let qualified = match &table.schema {
            Some(s) => format!("\"{}\".\"{}\"", s, table.name),
            None => format!("\"{}\"", table.name),
        };
        let rows = sqlx::query(
            "SELECT a.attname FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = to_regclass($1) AND i.indisprimary ORDER BY a.attnum",
        )
        .bind(&qualified)
        .fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
    }

    async fn function_definition(&self, name: &str) -> Result<String, AppError> {
        let def: Option<String> = sqlx::query_scalar(
            "SELECT pg_get_functiondef(p.oid) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') AND p.proname = $1 LIMIT 1",
        ).bind(name).fetch_optional(&self.pool).await?.flatten();
        Ok(def.unwrap_or_default())
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

    fn ddl_flavor(&self) -> crate::ddl::Flavor { crate::ddl::Flavor::Postgres }

    async fn list_databases(&self) -> Result<Vec<super::DbInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT datname, pg_database_size(datname) FROM pg_database \
             WHERE datistemplate = false AND datname <> 'postgres' ORDER BY datname",
        ).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| super::DbInfo {
            name: r.get::<String, _>(0),
            size_bytes: r.try_get::<i64, _>(1).ok(),
        }).collect())
    }
    async fn create_database(&self, name: &str) -> Result<(), AppError> {
        if !super::valid_db_name(name) { return Err(AppError::query("invalid database name")); }
        sqlx::query(&format!("CREATE DATABASE {}", quote(name))).execute(&self.pool).await?;
        Ok(())
    }
    async fn drop_database(&self, name: &str) -> Result<(), AppError> {
        if !super::valid_db_name(name) { return Err(AppError::query("invalid database name")); }
        sqlx::query(&format!("DROP DATABASE {}", quote(name))).execute(&self.pool).await?;
        Ok(())
    }

    async fn list_functions(&self) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT p.proname FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname NOT IN ('pg_catalog','information_schema') ORDER BY p.proname",
        ).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
    }

    async fn table_structure(&self, table: &TableInfo) -> Result<super::TableStructure, AppError> {
        let schema = table.schema.as_deref().unwrap_or("public");
        let pk = self.primary_key(table).await?;
        let qualified = match &table.schema {
            Some(s) => format!("\"{}\".\"{}\"", s, table.name),
            None => format!("\"{}\"", table.name),
        };

        // foreign keys: local column -> "ref_table(ref_col)"
        let fk_rows = sqlx::query(
            "SELECT kcu.column_name, ccu.table_name, ccu.column_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.constraint_column_usage ccu ON tc.constraint_name = ccu.constraint_name \
             WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2",
        )
        .bind(schema).bind(&table.name)
        .fetch_all(&self.pool).await?;
        let fks: std::collections::HashMap<String, String> = fk_rows.iter()
            .map(|r| (r.get::<String, _>(0), format!("{}({})", r.get::<String, _>(1), r.get::<String, _>(2))))
            .collect();

        let cols = sqlx::query(
            "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default, \
                    col_description(to_regclass($3)::oid, c.ordinal_position::int), c.is_generated \
             FROM information_schema.columns c WHERE c.table_schema = $1 AND c.table_name = $2 \
             ORDER BY c.ordinal_position",
        )
        .bind(schema).bind(&table.name).bind(&qualified)
        .fetch_all(&self.pool).await?;
        let columns = cols.iter().map(|r| {
            let name: String = r.get(0);
            super::StructColumn {
                is_pk: pk.contains(&name),
                foreign_key: fks.get(&name).cloned(),
                comment: r.try_get::<Option<String>, _>(4).unwrap_or(None),
                data_type: r.get::<String, _>(1),
                nullable: r.get::<String, _>(2) == "YES",
                default: r.try_get::<Option<String>, _>(3).unwrap_or(None),
                is_computed: r.get::<String, _>(5) != "NEVER",
                name,
            }
        }).collect();
        let idx = sqlx::query(
            "SELECT i.relname, a.attname, ix.indisunique \
             FROM pg_index ix \
             JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_class t ON t.oid = ix.indrelid \
             JOIN unnest(ix.indkey) WITH ORDINALITY AS x(attnum, ordinality) ON true \
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = x.attnum \
             WHERE t.oid = to_regclass($1) AND NOT ix.indisprimary \
             ORDER BY i.relname, x.ordinality",
        )
        .bind(&qualified)
        .fetch_all(&self.pool).await?;
        let mut indexes: Vec<super::StructIndex> = vec![];
        for r in &idx {
            let (name, col): (String, String) = (r.get(0), r.get(1));
            let unique: bool = r.get(2);
            match indexes.last_mut() {
                Some(last) if last.name == name => last.columns.push(col),
                _ => indexes.push(super::StructIndex { name, columns: vec![col], unique }),
            }
        }
        let trig = sqlx::query(
            "SELECT trigger_name, action_timing, event_manipulation FROM information_schema.triggers \
             WHERE event_object_schema = $1 AND event_object_table = $2 ORDER BY trigger_name",
        )
        .bind(schema).bind(&table.name)
        .fetch_all(&self.pool).await?;
        let triggers = trig.iter().map(|r| super::StructTrigger {
            name: r.get::<String, _>(0), timing: r.get::<String, _>(1), event: r.get::<String, _>(2),
        }).collect();

        let comment: Option<String> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT obj_description(to_regclass($1)::oid, 'pg_class')",
        )
        .bind(&qualified)
        .fetch_optional(&self.pool).await?
        .flatten();

        Ok(super::TableStructure { columns, indexes, triggers, comment })
    }
    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let pk = self.primary_key(table).await.unwrap_or_default();
        let (sql, binds) = build_select(table, opts, &pk, quote, cast_text, dollar, crate::changeset::escape_std);
        let mut q = sqlx::query(&sql);
        for b in &binds { q = q.bind(b); }
        let rows = q.fetch_all(&self.pool).await?;

        let columns: Vec<ColumnMeta> = match rows.first() {
            Some(r) => r.columns().iter()
                .map(|c| ColumnMeta { name: c.name().to_string(), data_type: c.type_info().to_string() })
                .collect(),
            None => sqlx::query(
                "SELECT column_name, data_type FROM information_schema.columns \
                 WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
            )
            .bind(table.schema.as_deref().unwrap_or("public")).bind(&table.name)
            .fetch_all(&self.pool).await?
            .iter()
            .map(|r| ColumnMeta { name: r.get::<String, _>(0), data_type: r.get::<String, _>(1) })
            .collect(),
        };

        let total: Option<i64> = sqlx::query_scalar::<_, f32>(
            "SELECT reltuples FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
        )
        .bind(table.schema.as_deref().unwrap_or("public")).bind(&table.name)
        .fetch_optional(&self.pool).await?
        .map(|estimate| estimate.max(0.0) as i64);

        Ok(RowsPage {
            query: sql,
            columns,
            rows: rows.iter().map(|r| (0..r.columns().len()).map(|i| cell(r, i)).collect()).collect(),
            total_estimate: total,
        })
    }
}

pub struct PgSession { conn: sqlx::pool::PoolConnection<sqlx::Postgres>, pid: i32 }

fn row_cells(row: &PgRow) -> Vec<serde_json::Value> {
    (0..row.columns().len()).map(|i| cell(row, i)).collect()
}
fn row_columns(row: &PgRow) -> Vec<ColumnMeta> {
    row.columns().iter()
        .map(|c| ColumnMeta { name: c.name().to_string(), data_type: c.type_info().to_string() })
        .collect()
}

#[async_trait::async_trait]
impl QuerySession for PgSession {
    fn session_id(&self) -> Option<String> { Some(self.pid.to_string()) }

    async fn stream(&mut self, sql: &str, tx: tokio::sync::mpsc::Sender<QueryEvent>) -> Result<(), AppError> {
        if super::is_unpreparable(sql) {
            use sqlx::Executor;
            // plain &str executes over the simple/text protocol (unprepared)
            let res = (&mut *self.conn).execute(sql).await.map_err(AppError::from)?;
            let _ = tx.send(QueryEvent::Done { row_count: 0, affected_rows: res.rows_affected() }).await;
            return Ok(());
        }
        let mut stream = sqlx::query(sql).fetch_many(&mut *self.conn);
        let mut sent_cols = false;
        let mut chunk: Vec<Vec<serde_json::Value>> = vec![];
        let (mut row_count, mut affected) = (0u64, 0u64);
        while let Some(item) = stream.try_next().await.map_err(AppError::from)? {
            match item {
                sqlx::Either::Left(res) => affected += res.rows_affected(),
                sqlx::Either::Right(row) => {
                    if !sent_cols {
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
