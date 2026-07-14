use super::{build_select, AppliedResult, ColumnMeta, FetchOptions, QueryEvent, QuerySession, RowsPage, SqlDriver, TableColumns, TableInfo, TableKind, STREAM_CHUNK};
use futures_util::TryStreamExt;
use crate::error::AppError;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow};
use sqlx::{Column, MySqlPool, Row};
use std::str::FromStr;

pub struct MysqlDriver { pool: MySqlPool, database: String }

fn quote(s: &str) -> String { format!("`{}`", s.replace('`', "``")) }
fn cast_text(quoted: &str) -> String { format!("CAST({quoted} AS CHAR)") }
fn qmark(_: usize) -> String { "?".into() }

impl MysqlDriver {
    pub async fn connect(host: &str, port: u16, user: &str, password: &str, db: &str) -> Result<Self, AppError> {
        let opts = MySqlConnectOptions::new()
            .host(host).port(port).username(user).password(password).database(db);
        Self::from_opts(opts, db).await
    }

    pub async fn connect_url(url: &str) -> Result<Self, AppError> {
        let opts = MySqlConnectOptions::from_str(url).map_err(|e| AppError::connection(e.to_string()))?;
        let db = opts.get_database().unwrap_or_default().to_string();
        Self::from_opts(opts, &db).await
    }

    async fn from_opts(opts: MySqlConnectOptions, db: &str) -> Result<Self, AppError> {
        let pool = MySqlPoolOptions::new().max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(8))
            // Reconnect transparently after server-side idle timeouts: ping stale
            // connections before handing them out, recycle idle/old ones proactively.
            .test_before_acquire(true)
            .idle_timeout(std::time::Duration::from_secs(600))
            .max_lifetime(std::time::Duration::from_secs(1800))
            .connect_with(opts).await?;
        Ok(Self { pool, database: db.to_string() })
    }

}

pub(crate) fn cell(row: &MySqlRow, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    use sqlx::ValueRef as _;
    if let Ok(raw) = row.try_get_raw(i) {
        if raw.is_null() { return serde_json::Value::Null; }
    }
    if let Ok(v) = row.try_get::<i64, _>(i) { return super::int64_value(v); }
    if let Ok(v) = row.try_get::<u64, _>(i) { return super::uint64_value(v); }
    if let Ok(v) = row.try_get::<f64, _>(i) { return super::float_value(v); }
    if let Ok(v) = row.try_get::<f32, _>(i) { return super::float_value(v as f64); }
    if let Ok(v) = row.try_get::<String, _>(i) { return v.into(); }
    if let Ok(v) = row.try_get::<sqlx::types::Decimal, _>(i) { return v.to_string().into(); } // exact, not lossy f64
    // DATETIME is zoneless: prefer naive display over a fabricated +00:00 offset
    if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) { return v.to_rfc3339().into(); }
    if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(i) { return v.to_string().into(); }
    if let Ok(v) = row.try_get::<serde_json::Value, _>(i) { return v; }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(i) {
        return base64::engine::general_purpose::STANDARD.encode(v).into();
    }
    serde_json::Value::String(format!("\u{27e8}{}\u{27e9}", row.column(i).type_info()))
}

#[async_trait::async_trait]
impl SqlDriver for MysqlDriver {
    async fn ping(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT CAST(table_name AS CHAR) AS table_name, CAST(table_type AS CHAR) AS table_type FROM information_schema.tables \
             WHERE table_schema = ? ORDER BY table_name",
        )
        .bind(&self.database)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: None, // single-database connection; schema qualifier unnecessary
            name: r.get::<String, _>(0),
            kind: if r.get::<String, _>(1) == "VIEW" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn columns_meta(&self) -> Result<Vec<TableColumns>, AppError> {
        let rows = sqlx::query(
            "SELECT CAST(table_name AS CHAR) AS t, CAST(column_name AS CHAR) AS c \
             FROM information_schema.columns WHERE table_schema = ? \
             ORDER BY table_name, ordinal_position",
        ).bind(&self.database).fetch_all(&self.pool).await?;
        let mut out: Vec<TableColumns> = vec![];
        for r in rows {
            let (t, c): (String, String) = (r.get(0), r.get(1));
            match out.last_mut() {
                Some(last) if last.table == t => last.columns.push(c),
                _ => out.push(TableColumns { schema: None, table: t, columns: vec![c] }),
            }
        }
        Ok(out)
    }

    async fn acquire_session(&self) -> Result<Box<dyn QuerySession>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()").fetch_one(&mut *conn).await?;
        Ok(Box::new(MysqlSession { conn, id }))
    }

    async fn kill_session(&self, session_id: &str) -> Result<(), AppError> {
        let id: u64 = session_id.parse().map_err(|_| AppError::internal("bad session id"))?;
        sqlx::query(&format!("KILL QUERY {id}")).execute(&self.pool).await?;
        Ok(())
    }


    async fn primary_key(&self, table: &TableInfo) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT CAST(column_name AS CHAR) FROM information_schema.key_column_usage \
             WHERE table_schema = ? AND table_name = ? AND constraint_name = 'PRIMARY' \
             ORDER BY ordinal_position",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
    }

    async fn function_definition(&self, name: &str) -> Result<String, AppError> {
        let def: Option<String> = sqlx::query_scalar(
            "SELECT CAST(routine_definition AS CHAR) FROM information_schema.routines \
             WHERE routine_schema = ? AND routine_name = ? LIMIT 1",
        ).bind(&self.database).bind(name).fetch_optional(&self.pool).await?.flatten();
        Ok(def.unwrap_or_default())
    }

    fn preview_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Vec<String> {
        crate::changeset::render_changeset(table, cs, quote, crate::changeset::escape_mysql)
            .into_iter().map(|s| s.sql).collect()
    }

    async fn apply_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Result<AppliedResult, AppError> {
        let stmts = crate::changeset::render_changeset(table, cs, quote, crate::changeset::escape_mysql);
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

    fn ddl_flavor(&self) -> crate::ddl::Flavor { crate::ddl::Flavor::Mysql }

    async fn list_databases(&self) -> Result<Vec<super::DbInfo>, AppError> {
        let rows = sqlx::query(
            "SELECT s.schema_name, CAST(COALESCE(t.sz, 0) AS SIGNED) \
             FROM information_schema.schemata s \
             LEFT JOIN (SELECT table_schema, SUM(data_length + index_length) sz \
                        FROM information_schema.tables GROUP BY table_schema) t \
               ON t.table_schema = s.schema_name \
             WHERE s.schema_name NOT IN ('information_schema','performance_schema','mysql','sys') \
             ORDER BY s.schema_name",
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
            "SELECT CAST(routine_name AS CHAR) FROM information_schema.routines \
             WHERE routine_schema = ? ORDER BY routine_name",
        ).bind(&self.database).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
    }

    async fn table_structure(&self, table: &TableInfo) -> Result<super::TableStructure, AppError> {
        let fk_rows = sqlx::query(
            "SELECT CAST(column_name AS CHAR), CAST(referenced_table_name AS CHAR), CAST(referenced_column_name AS CHAR) \
             FROM information_schema.key_column_usage \
             WHERE table_schema = ? AND table_name = ? AND referenced_table_name IS NOT NULL",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_all(&self.pool).await?;
        let fks: std::collections::HashMap<String, String> = fk_rows.iter()
            .map(|r| (r.get::<String, _>(0), format!("{}({})", r.get::<String, _>(1), r.get::<String, _>(2))))
            .collect();

        let cols = sqlx::query(
            "SELECT CAST(column_name AS CHAR), CAST(column_type AS CHAR), CAST(is_nullable AS CHAR), \
                    CAST(column_default AS CHAR), CAST(column_key AS CHAR), CAST(column_comment AS CHAR), \
                    CAST(extra AS CHAR) \
             FROM information_schema.columns WHERE table_schema = ? AND table_name = ? \
             ORDER BY ordinal_position",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_all(&self.pool).await?;
        let columns = cols.iter().map(|r| {
            let name: String = r.get(0);
            let comment: String = r.get(5);
            super::StructColumn {
                foreign_key: fks.get(&name).cloned(),
                comment: if comment.is_empty() { None } else { Some(comment) },
                data_type: r.get::<String, _>(1),
                nullable: r.get::<String, _>(2) == "YES",
                default: r.try_get::<Option<String>, _>(3).unwrap_or(None),
                is_pk: r.get::<String, _>(4) == "PRI",
                is_computed: r.get::<String, _>(6).contains("GENERATED"),
                name,
            }
        }).collect();

        let idx = sqlx::query(
            "SELECT CAST(index_name AS CHAR), CAST(column_name AS CHAR), non_unique \
             FROM information_schema.statistics \
             WHERE table_schema = ? AND table_name = ? AND index_name != 'PRIMARY' \
             ORDER BY index_name, seq_in_index",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_all(&self.pool).await?;
        let mut indexes: Vec<super::StructIndex> = vec![];
        for r in &idx {
            let (name, col): (String, String) = (r.get(0), r.get(1));
            let non_unique: i64 = r.get(2);
            match indexes.last_mut() {
                Some(last) if last.name == name => last.columns.push(col),
                _ => indexes.push(super::StructIndex { name, columns: vec![col], unique: non_unique == 0 }),
            }
        }
        let trig = sqlx::query(
            "SELECT CAST(trigger_name AS CHAR), CAST(action_timing AS CHAR), CAST(event_manipulation AS CHAR) \
             FROM information_schema.triggers \
             WHERE event_object_schema = ? AND event_object_table = ? ORDER BY trigger_name",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_all(&self.pool).await?;
        let triggers = trig.iter().map(|r| super::StructTrigger {
            name: r.get::<String, _>(0), timing: r.get::<String, _>(1), event: r.get::<String, _>(2),
        }).collect();

        let comment: Option<String> = sqlx::query_scalar::<_, String>(
            "SELECT CAST(table_comment AS CHAR) FROM information_schema.tables \
             WHERE table_schema = ? AND table_name = ?",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_optional(&self.pool).await?
        .filter(|c| !c.is_empty());

        Ok(super::TableStructure { columns, indexes, triggers, comment })
    }
    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let (sql, binds) = build_select(table, opts, quote, cast_text, qmark);
        let mut q = sqlx::query(&sql);
        for b in &binds { q = q.bind(b); }
        let rows = q.fetch_all(&self.pool).await?;

        let columns: Vec<ColumnMeta> = match rows.first() {
            Some(r) => r.columns().iter()
                .map(|c| ColumnMeta { name: c.name().to_string(), data_type: c.type_info().to_string() })
                .collect(),
            None => sqlx::query(
                "SELECT CAST(column_name AS CHAR), CAST(column_type AS CHAR) \
                 FROM information_schema.columns WHERE table_schema = ? AND table_name = ? \
                 ORDER BY ordinal_position",
            )
            .bind(&self.database).bind(&table.name)
            .fetch_all(&self.pool).await?
            .iter()
            .map(|r| ColumnMeta { name: r.get::<String, _>(0), data_type: r.get::<String, _>(1) })
            .collect(),
        };

        // information_schema.table_rows is an estimate on InnoDB -- what the spec wants for big tables
        let total: Option<i64> = sqlx::query_scalar(
            "SELECT CAST(table_rows AS SIGNED) FROM information_schema.tables WHERE table_schema = ? AND table_name = ?",
        )
        .bind(&self.database).bind(&table.name)
        .fetch_optional(&self.pool).await?
        .flatten();

        Ok(RowsPage {
            query: sql,
            columns,
            rows: rows.iter().map(|r| (0..r.columns().len()).map(|i| cell(r, i)).collect()).collect(),
            total_estimate: total,
        })
    }
}

pub struct MysqlSession { conn: sqlx::pool::PoolConnection<sqlx::MySql>, id: u64 }

fn row_cells(row: &MySqlRow) -> Vec<serde_json::Value> {
    (0..row.columns().len()).map(|i| cell(row, i)).collect()
}
fn row_columns(row: &MySqlRow) -> Vec<ColumnMeta> {
    row.columns().iter()
        .map(|c| ColumnMeta { name: c.name().to_string(), data_type: c.type_info().to_string() })
        .collect()
}

#[async_trait::async_trait]
impl QuerySession for MysqlSession {
    fn session_id(&self) -> Option<String> { Some(self.id.to_string()) }

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
