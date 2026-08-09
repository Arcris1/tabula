use super::{
    AppliedResult, ColumnMeta, FetchOptions, FilterOp, QueryEvent, QuerySession, RowsPage,
    SqlDriver, TableColumns, TableInfo, TableKind, STREAM_CHUNK,
};
use crate::error::AppError;
use futures_util::TryStreamExt;
use tiberius::{AuthMethod, Config, Row, ToSql};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type Manager = bb8_tiberius::ConnectionManager;
type Pool = bb8::Pool<Manager>;
/// A dedicated (non-pooled) connection used for per-tab query sessions.
type SessionClient = tiberius::Client<Compat<tokio::net::TcpStream>>;

pub struct MssqlDriver {
    pool: Pool,
    config: Config,
}

fn quote(s: &str) -> String {
    format!("[{}]", s.replace(']', "]]"))
}

fn build_config(host: &str, port: u16, user: &str, password: &str, db: &str) -> Config {
    let mut config = Config::new();
    config.host(host);
    config.port(port);
    config.authentication(AuthMethod::sql_server(user, password));
    if !db.is_empty() {
        config.database(db);
    }
    // dev tool: accept self-signed server certs (encryption still negotiated)
    config.trust_cert();
    config
}

async fn connect_client(config: &Config) -> Result<SessionClient, AppError> {
    let tcp = tokio::net::TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true).ok();
    let client = tiberius::Client::connect(config.clone(), tcp.compat_write()).await?;
    Ok(client)
}

impl MssqlDriver {
    pub async fn connect(host: &str, port: u16, user: &str, password: &str, db: &str) -> Result<Self, AppError> {
        let config = build_config(host, port, user, password, db);
        // Probe with a DIRECT connection first: bb8's pool.get() retries failed
        // connects until its timeout and then reports only "timed out", hiding
        // the real cause (bad login, unknown database, TLS). The probe surfaces
        // the genuine error immediately.
        let probe = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            connect_client(&config),
        )
        .await
        .map_err(|_| AppError::timeout(format!("could not reach SQL Server at {host}:{port} within 8s")))??;
        drop(probe);

        let mgr = Manager::build(config.clone()).map_err(AppError::from)?;
        let pool = bb8::Pool::builder()
            .max_size(10)
            .connection_timeout(std::time::Duration::from_secs(8))
            .build(mgr)
            .await
            .map_err(AppError::from)?;
        Ok(Self { pool, config })
    }

    async fn query_rows(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>, AppError> {
        let mut conn = self.pool.get().await?;
        let rows = conn.query(sql, params).await?.into_first_result().await?;
        Ok(rows)
    }
}

/// unqualified `schema.table` reference for OBJECT_ID(...)
fn object_ref(table: &TableInfo) -> String {
    match &table.schema {
        Some(s) => format!("{}.{}", s, table.name),
        None => table.name.clone(),
    }
}

fn get_str(r: &Row, col: &str) -> String {
    r.try_get::<&str, _>(col).ok().flatten().unwrap_or_default().to_string()
}
fn get_i32(r: &Row, col: &str) -> i32 {
    r.try_get::<i32, _>(col).ok().flatten().unwrap_or(0)
}

/// Maps a single cell to JSON. tiberius decodes each column into exactly one
/// value variant, so we try candidate Rust types in order: the matching one
/// returns Ok(Some)=value or Ok(None)=NULL; all others return Err and are skipped.
fn cell(row: &Row, i: usize) -> serde_json::Value {
    use base64::Engine as _;
    macro_rules! try_ty {
        ($ty:ty => $conv:expr) => {
            match row.try_get::<$ty, _>(i) {
                Ok(Some(v)) => return $conv(v),
                Ok(None) => return serde_json::Value::Null,
                Err(_) => {}
            }
        };
    }
    try_ty!(i32 => |v: i32| serde_json::Value::from(v));
    try_ty!(i64 => |v: i64| super::int64_value(v));
    try_ty!(i16 => |v: i16| serde_json::Value::from(v));
    try_ty!(u8 => |v: u8| serde_json::Value::from(v));
    // bit -> 0/1 (SQL Server convention; also avoids TRUE/FALSE literals on edit)
    try_ty!(bool => |v: bool| serde_json::Value::from(if v { 1 } else { 0 }));
    try_ty!(f32 => |v: f32| super::float_value(v as f64));
    try_ty!(f64 => |v: f64| super::float_value(v));
    try_ty!(rust_decimal::Decimal => |v: rust_decimal::Decimal| serde_json::Value::from(v.to_string()));
    try_ty!(&str => |v: &str| serde_json::Value::from(v.to_string()));
    try_ty!(uuid::Uuid => |v: uuid::Uuid| serde_json::Value::from(v.to_string()));
    try_ty!(chrono::NaiveDateTime => |v: chrono::NaiveDateTime| serde_json::Value::from(v.to_string()));
    try_ty!(chrono::NaiveDate => |v: chrono::NaiveDate| serde_json::Value::from(v.to_string()));
    try_ty!(chrono::NaiveTime => |v: chrono::NaiveTime| serde_json::Value::from(v.to_string()));
    try_ty!(chrono::DateTime<chrono::Utc> => |v: chrono::DateTime<chrono::Utc>| serde_json::Value::from(v.to_rfc3339()));
    try_ty!(chrono::DateTime<chrono::FixedOffset> => |v: chrono::DateTime<chrono::FixedOffset>| serde_json::Value::from(v.to_rfc3339()));
    try_ty!(&[u8] => |v: &[u8]| serde_json::Value::from(base64::engine::general_purpose::STANDARD.encode(v)));
    // visible marker: never render an undecodable value as a fake NULL
    serde_json::Value::String(format!(
        "\u{27e8}{:?}\u{27e9}",
        row.columns().get(i).map(|c| c.column_type())
    ))
}

fn row_columns(row: &Row) -> Vec<ColumnMeta> {
    row.columns()
        .iter()
        .map(|c| ColumnMeta { name: c.name().to_string(), data_type: format!("{:?}", c.column_type()) })
        .collect()
}
fn row_cells(row: &Row) -> Vec<serde_json::Value> {
    (0..row.columns().len()).map(|i| cell(row, i)).collect()
}

/// SELECT with filters/sort/paging. T-SQL paging (OFFSET/FETCH) requires an
/// ORDER BY, so we fall back to the primary key, then to a stable no-op order.
fn build_select(table: &TableInfo, opts: &FetchOptions, pk: &[String]) -> (String, Vec<String>) {
    let qualified = super::qualified_name(table, quote);
    let mut sql = format!("SELECT * FROM {qualified}");
    let mut binds: Vec<String> = vec![];
    let mut clauses: Vec<String> = vec![];
    for f in &opts.filters {
        let col = quote(&f.column);
        match f.op {
            FilterOp::Contains => {
                // escape LIKE wildcards — T-SQL also treats `[` as a class opener
                let esc = super::escape_like(&f.value).replace('[', "![");
                binds.push(format!("%{esc}%"));
                clauses.push(format!("CAST({col} AS NVARCHAR(MAX)) LIKE @P{} ESCAPE '!'", binds.len()));
            }
            FilterOp::Equals => {
                // inline escaped literal so the server coerces it to the column
                // type — CAST(col AS NVARCHAR(MAX)) = @P forced a full scan
                clauses.push(format!("{} = '{}'", col, crate::changeset::escape_std(&f.value)));
            }
            FilterOp::IsNull => clauses.push(format!("{col} IS NULL")),
            FilterOp::NotNull => clauses.push(format!("{col} IS NOT NULL")),
        }
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    // user sort + PK tiebreaker (stable paging on non-unique sort columns)
    let mut order: Vec<String> = vec![];
    if let Some(sort) = &opts.sort {
        order.push(format!("{} {}", quote(&sort.column), if sort.desc { "DESC" } else { "ASC" }));
        order.extend(pk.iter().filter(|k| **k != sort.column).map(|k| quote(k)));
    } else {
        order.extend(pk.iter().map(|k| quote(k)));
    }
    if order.is_empty() {
        order.push("(SELECT NULL)".to_string()); // OFFSET/FETCH requires ORDER BY
    }
    sql.push_str(&format!(
        " ORDER BY {} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
        order.join(", "),
        opts.offset, opts.limit
    ));
    (sql, binds)
}

#[async_trait::async_trait]
impl SqlDriver for MssqlDriver {
    async fn ping(&self) -> Result<(), AppError> {
        let mut conn = self.pool.get().await?;
        conn.query("SELECT 1", &[]).await?.into_first_result().await?;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, AppError> {
        let rows = self.query_rows(
            "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM INFORMATION_SCHEMA.TABLES \
             ORDER BY TABLE_SCHEMA, TABLE_NAME",
            &[],
        ).await?;
        Ok(rows.iter().map(|r| TableInfo {
            schema: Some(get_str(r, "TABLE_SCHEMA")),
            name: get_str(r, "TABLE_NAME"),
            kind: if get_str(r, "TABLE_TYPE") == "VIEW" { TableKind::View } else { TableKind::Table },
        }).collect())
    }

    async fn columns_meta(&self) -> Result<Vec<TableColumns>, AppError> {
        let rows = self.query_rows(
            "SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS \
             ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION",
            &[],
        ).await?;
        let mut out: Vec<TableColumns> = vec![];
        for r in &rows {
            let s = Some(get_str(r, "TABLE_SCHEMA"));
            let t = get_str(r, "TABLE_NAME");
            let c = get_str(r, "COLUMN_NAME");
            match out.last_mut() {
                Some(last) if last.table == t && last.schema == s => last.columns.push(c),
                _ => out.push(TableColumns { schema: s, table: t, columns: vec![c] }),
            }
        }
        Ok(out)
    }

    async fn acquire_session(&self) -> Result<Box<dyn QuerySession>, AppError> {
        let mut client = connect_client(&self.config).await?;
        let spid = client.query("SELECT @@SPID", &[]).await?
            .into_row().await?
            .and_then(|r| r.try_get::<i16, _>(0).ok().flatten())
            .map(|s| s.to_string());
        Ok(Box::new(MssqlSession { client, spid }))
    }

    async fn kill_session(&self, session_id: &str) -> Result<(), AppError> {
        let spid: i32 = session_id.parse().map_err(|_| AppError::internal("bad session id"))?;
        let mut conn = self.pool.get().await?;
        // KILL takes no parameters; spid is a validated integer
        conn.execute(format!("KILL {spid}"), &[]).await?;
        Ok(())
    }

    async fn primary_key(&self, table: &TableInfo) -> Result<Vec<String>, AppError> {
        let rows = self.query_rows(
            "SELECT c.name AS col FROM sys.indexes i \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             WHERE i.is_primary_key = 1 AND i.object_id = OBJECT_ID(@P1) \
             ORDER BY ic.key_ordinal",
            &[&object_ref(table)],
        ).await?;
        Ok(rows.iter().map(|r| get_str(r, "col")).collect())
    }

    fn preview_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Vec<String> {
        crate::changeset::render_changeset(table, cs, quote, crate::changeset::escape_std)
            .into_iter().map(|s| s.sql).collect()
    }

    async fn apply_changeset(&self, table: &TableInfo, cs: &crate::changeset::Changeset) -> Result<AppliedResult, AppError> {
        use crate::changeset::StatementKind;
        let stmts = crate::changeset::render_changeset(table, cs, quote, crate::changeset::escape_std);
        if stmts.is_empty() { return Ok(AppliedResult::default()); }

        // One balanced batch (BEGIN…COMMIT within a single statement) so @@TRANCOUNT
        // returns to 0 at batch exit — separate BEGIN/COMMIT executes trip error 266.
        // Optimistic concurrency is enforced server-side: any UPDATE/DELETE that
        // doesn't hit exactly one row rolls back and THROWs a conflict.
        let mut batch = String::from("SET XACT_ABORT ON;\nBEGIN TRAN;\n");
        let mut res = AppliedResult::default();
        for s in &stmts {
            batch.push_str(&s.sql);
            batch.push_str(";\n");
            if s.expect_one {
                let msg = format!("row changed externally ({})", s.pk_desc).replace('\'', "''");
                batch.push_str(&format!(
                    "IF @@ROWCOUNT <> 1 BEGIN ROLLBACK TRAN; THROW 50000, '{msg}', 1; END\n"
                ));
            }
            match s.kind {
                StatementKind::Update => res.updated += 1,
                StatementKind::Insert => res.inserted += 1,
                StatementKind::Delete => res.deleted += 1,
            }
        }
        batch.push_str("COMMIT TRAN;");

        let mut conn = self.pool.get().await?;
        match conn.execute(batch.as_str(), &[]).await {
            Ok(_) => Ok(res),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("row changed externally") {
                    Err(AppError::conflict(msg))
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn execute_raw(&self, sql: &str) -> Result<(), AppError> {
        let mut conn = self.pool.get().await?;
        conn.execute(sql, &[]).await?;
        Ok(())
    }

    fn ddl_flavor(&self) -> crate::ddl::Flavor { crate::ddl::Flavor::Mssql }

    async fn list_databases(&self) -> Result<Vec<super::DbInfo>, AppError> {
        let rows = self.query_rows(
            "SELECT d.name AS name, \
                    (SELECT SUM(CAST(mf.size AS BIGINT)) * 8 * 1024 FROM sys.master_files mf \
                     WHERE mf.database_id = d.database_id) AS size \
             FROM sys.databases d WHERE d.database_id > 4 ORDER BY d.name",
            &[],
        ).await?;
        Ok(rows.iter().map(|r| super::DbInfo {
            name: get_str(r, "name"),
            size_bytes: r.try_get::<i64, _>("size").ok().flatten(),
        }).collect())
    }
    async fn create_database(&self, name: &str) -> Result<(), AppError> {
        if !super::valid_db_name(name) { return Err(AppError::query("invalid database name")); }
        let mut conn = self.pool.get().await?;
        conn.execute(format!("CREATE DATABASE {}", quote(name)), &[]).await?;
        Ok(())
    }
    async fn drop_database(&self, name: &str) -> Result<(), AppError> {
        if !super::valid_db_name(name) { return Err(AppError::query("invalid database name")); }
        let mut conn = self.pool.get().await?;
        conn.execute(format!("DROP DATABASE {}", quote(name)), &[]).await?;
        Ok(())
    }

    async fn list_functions(&self) -> Result<Vec<String>, AppError> {
        let rows = self.query_rows(
            "SELECT name FROM sys.objects WHERE type IN ('FN','IF','TF','P') AND is_ms_shipped = 0 \
             ORDER BY name",
            &[],
        ).await?;
        Ok(rows.iter().map(|r| get_str(r, "name")).collect())
    }

    async fn function_definition(&self, name: &str) -> Result<String, AppError> {
        let rows = self.query_rows("SELECT OBJECT_DEFINITION(OBJECT_ID(@P1)) AS def", &[&name.to_string()]).await?;
        Ok(rows.first().map(|r| get_str(r, "def")).unwrap_or_default())
    }

    async fn table_structure(&self, table: &TableInfo) -> Result<super::TableStructure, AppError> {
        let oref = object_ref(table);
        let schema = table.schema.clone().unwrap_or_else(|| "dbo".into());
        let pk = self.primary_key(table).await?;

        // foreign keys: local column -> "ref_table(ref_col)"
        let fk_rows = self.query_rows(
            "SELECT pc.name AS col, rt.name AS ref_table, rc.name AS ref_col \
             FROM sys.foreign_key_columns fkc \
             JOIN sys.columns pc ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id \
             JOIN sys.tables rt ON rt.object_id = fkc.referenced_object_id \
             JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id \
             WHERE fkc.parent_object_id = OBJECT_ID(@P1)",
            &[&oref],
        ).await?;
        let fks: std::collections::HashMap<String, String> = fk_rows.iter()
            .map(|r| (get_str(r, "col"), format!("{}({})", get_str(r, "ref_table"), get_str(r, "ref_col"))))
            .collect();

        let col_rows = self.query_rows(
            "SELECT c.COLUMN_NAME, c.DATA_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, \
                    COLUMNPROPERTY(OBJECT_ID(@P1), c.COLUMN_NAME, 'IsComputed') AS is_computed \
             FROM INFORMATION_SCHEMA.COLUMNS c \
             WHERE c.TABLE_SCHEMA = @P2 AND c.TABLE_NAME = @P3 \
             ORDER BY c.ORDINAL_POSITION",
            &[&oref, &schema, &table.name],
        ).await?;
        let columns = col_rows.iter().map(|r| {
            let name = get_str(r, "COLUMN_NAME");
            super::StructColumn {
                is_pk: pk.contains(&name),
                foreign_key: fks.get(&name).cloned(),
                comment: None,
                data_type: get_str(r, "DATA_TYPE"),
                nullable: get_str(r, "IS_NULLABLE") == "YES",
                default: r.try_get::<&str, _>("COLUMN_DEFAULT").ok().flatten().map(|s| s.to_string()),
                is_computed: get_i32(r, "is_computed") != 0,
                name,
            }
        }).collect();

        let idx_rows = self.query_rows(
            "SELECT i.name AS idx, c.name AS col, i.is_unique AS is_unique \
             FROM sys.indexes i \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             WHERE i.object_id = OBJECT_ID(@P1) AND i.is_primary_key = 0 AND i.name IS NOT NULL \
             ORDER BY i.name, ic.key_ordinal",
            &[&oref],
        ).await?;
        let mut indexes: Vec<super::StructIndex> = vec![];
        for r in &idx_rows {
            let name = get_str(r, "idx");
            let col = get_str(r, "col");
            let unique = r.try_get::<bool, _>("is_unique").ok().flatten().unwrap_or(false);
            match indexes.last_mut() {
                Some(last) if last.name == name => last.columns.push(col),
                _ => indexes.push(super::StructIndex { name, columns: vec![col], unique }),
            }
        }

        let trig_rows = self.query_rows(
            "SELECT tr.name AS name, tr.is_instead_of_trigger AS instead_of \
             FROM sys.triggers tr WHERE tr.parent_id = OBJECT_ID(@P1) ORDER BY tr.name",
            &[&oref],
        ).await?;
        let triggers = trig_rows.iter().map(|r| super::StructTrigger {
            name: get_str(r, "name"),
            timing: if r.try_get::<bool, _>("instead_of").ok().flatten().unwrap_or(false) { "INSTEAD OF".into() } else { "AFTER".into() },
            event: String::new(),
        }).collect();

        Ok(super::TableStructure { columns, indexes, triggers, comment: None })
    }

    async fn fetch_rows(&self, table: &TableInfo, opts: &FetchOptions) -> Result<RowsPage, AppError> {
        let pk = self.primary_key(table).await.unwrap_or_default();
        let (sql, binds) = build_select(table, opts, &pk);
        let params: Vec<&dyn ToSql> = binds.iter().map(|b| b as &dyn ToSql).collect();
        let rows = self.query_rows(&sql, &params).await?;

        let columns: Vec<ColumnMeta> = match rows.first() {
            Some(r) => row_columns(r),
            None => {
                let schema = table.schema.clone().unwrap_or_else(|| "dbo".into());
                self.query_rows(
                    "SELECT COLUMN_NAME, DATA_TYPE FROM INFORMATION_SCHEMA.COLUMNS \
                     WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2 ORDER BY ORDINAL_POSITION",
                    &[&schema, &table.name],
                ).await?
                .iter()
                .map(|r| ColumnMeta { name: get_str(r, "COLUMN_NAME"), data_type: get_str(r, "DATA_TYPE") })
                .collect()
            }
        };

        let total: Option<i64> = self.query_rows(
            "SELECT CAST(SUM(p.rows) AS BIGINT) AS n FROM sys.partitions p \
             WHERE p.object_id = OBJECT_ID(@P1) AND p.index_id IN (0,1)",
            &[&object_ref(table)],
        ).await?.first().and_then(|r| r.try_get::<i64, _>("n").ok().flatten());

        Ok(RowsPage {
            query: sql,
            columns,
            rows: rows.iter().map(row_cells).collect(),
            total_estimate: total,
        })
    }
}

/// tiberius never exposes PRINT / server info messages in its API — it only
/// logs them (`event!(Level::INFO, "{}", info.message)` in tds/stream/token.rs).
/// We capture those tracing events, correlated to the running query by a span
/// carrying a query-scoped id, and forward them as QueryEvent::Message.
pub(crate) mod msgs {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    fn sinks() -> &'static Mutex<HashMap<u64, Vec<String>>> {
        static S: OnceLock<Mutex<HashMap<u64, Vec<String>>>> = OnceLock::new();
        S.get_or_init(|| Mutex::new(HashMap::new()))
    }
    pub fn register() -> u64 {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        sinks().lock().unwrap().insert(id, vec![]);
        id
    }
    pub fn push(id: u64, m: String) {
        if let Some(v) = sinks().lock().unwrap().get_mut(&id) {
            if v.len() < 500 { v.push(m); } // bound: runaway PRINT loops can't grow unchecked
        }
    }
    pub fn take(id: u64) -> Vec<String> {
        sinks().lock().unwrap().remove(&id).unwrap_or_default()
    }
}

/// tracing Layer that collects tiberius token-stream INFO events (PRINT output,
/// "Changed database context…" env changes) into the enclosing query's sink.
pub struct TiberiusMessageLayer;

struct QidTag(u64);

impl<S> tracing_subscriber::Layer<S> for TiberiusMessageLayer
where
    S: tracing::Subscriber + for<'l> tracing_subscriber::registry::LookupSpan<'l>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() != "mssql_msgs" { return; }
        struct V(Option<u64>);
        impl tracing::field::Visit for V {
            fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
                if f.name() == "qid" { self.0 = Some(v); }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut v = V(None);
        attrs.record(&mut v);
        if let (Some(q), Some(span)) = (v.0, ctx.span(id)) {
            span.extensions_mut().insert(QidTag(q));
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // only the token-stream module logs server messages; connect-time noise
        // (TLS handshake) lives elsewhere and outside any query span anyway
        if *event.metadata().level() != tracing::Level::INFO
            || !event.metadata().target().starts_with("tiberius")
        {
            return;
        }
        let Some(scope) = ctx.event_scope(event) else { return };
        let mut qid = None;
        for span in scope {
            if let Some(t) = span.extensions().get::<QidTag>() { qid = Some(t.0); break; }
        }
        let Some(qid) = qid else { return };
        struct M(Option<String>);
        impl tracing::field::Visit for M {
            fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
                // the "message" field is fmt::Arguments: Debug == Display, no quoting
                if f.name() == "message" { self.0 = Some(format!("{v:?}")); }
            }
        }
        let mut m = M(None);
        event.record(&mut m);
        if let Some(msg) = m.0 { msgs::push(qid, msg); }
    }
}

pub struct MssqlSession {
    client: SessionClient,
    spid: Option<String>,
}

#[async_trait::async_trait]
impl QuerySession for MssqlSession {
    fn session_id(&self) -> Option<String> { self.spid.clone() }

    async fn stream(&mut self, sql: &str, tx: tokio::sync::mpsc::Sender<QueryEvent>) -> Result<(), AppError> {
        use tracing::Instrument as _;
        // run the batch inside a qid-tagged span so TiberiusMessageLayer can
        // attribute PRINT/info messages emitted while polling this future
        let qid = msgs::register();
        let span = tracing::info_span!("mssql_msgs", qid = qid);
        let res = self.stream_impl(sql, &tx).instrument(span).await;
        for m in msgs::take(qid) {
            let _ = tx.send(QueryEvent::Message(m)).await;
        }
        res
    }
}

/// Position just past leading whitespace and `--` / `/* */` comments.
fn skip_ws_comments(b: &[u8], mut i: usize) -> usize {
    loop {
        while i < b.len() && (b[i] as char).is_ascii_whitespace() { i += 1; }
        if i + 1 < b.len() && b[i] == b'-' && b[i + 1] == b'-' {
            while i < b.len() && b[i] != b'\n' { i += 1; }
        } else if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') { i += 1; }
            i = (i + 2).min(b.len());
        } else {
            return i;
        }
    }
}

fn read_word(b: &[u8], i: usize) -> (String, usize) {
    let mut j = i;
    while j < b.len() && ((b[j] as char).is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'@' || b[j] == b'#') {
        j += 1;
    }
    (String::from_utf8_lossy(&b[i..j]).to_ascii_lowercase(), j)
}

/// Skip a balanced `( … )` group, ignoring parens inside 'strings'.
fn skip_parens(b: &[u8], mut i: usize) -> usize {
    debug_assert!(b.get(i) == Some(&b'('));
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                i += 1;
                while i < b.len() {
                    if b[i] == b'\'' {
                        if b.get(i + 1) == Some(&b'\'') { i += 2; continue; }
                        break;
                    }
                    i += 1;
                }
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 { return i + 1; }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// Whether a batch should run through execute() to get accurate affected-row
/// counts: plain DML, or CTE-led DML (`WITH … UPDATE/DELETE/INSERT/MERGE`).
/// Skips leading comments so `-- JIRA-123` annotations don't defeat detection.
fn is_dml_batch(sql: &str) -> bool {
    let b = sql.as_bytes();
    let i = skip_ws_comments(b, 0);
    let (kw, mut i) = read_word(b, i);
    match kw.as_str() {
        "insert" | "update" | "delete" | "merge" => true,
        "with" => {
            // walk the CTE list: name [(cols)] AS ( … ) [, …] then peek the verb
            loop {
                i = skip_ws_comments(b, i);
                let (_name, j) = read_word(b, i);
                if j == i { return false; } // malformed — be conservative
                i = skip_ws_comments(b, j);
                if b.get(i) == Some(&b'(') {
                    i = skip_parens(b, i); // optional column list
                    i = skip_ws_comments(b, i);
                }
                let (as_kw, j) = read_word(b, i);
                if as_kw != "as" { return false; }
                i = skip_ws_comments(b, j);
                if b.get(i) != Some(&b'(') { return false; }
                i = skip_parens(b, i); // the CTE body
                i = skip_ws_comments(b, i);
                if b.get(i) == Some(&b',') { i += 1; continue; }
                let (verb, _) = read_word(b, i);
                return matches!(verb.as_str(), "insert" | "update" | "delete" | "merge");
            }
        }
        _ => false,
    }
}

impl MssqlSession {
    async fn stream_impl(&mut self, sql: &str, tx: &tokio::sync::mpsc::Sender<QueryEvent>) -> Result<(), AppError> {
        // DML goes through execute() for accurate affected-row counts.
        // EVERYTHING else must be a raw SQL batch (simple_query): execute()'s
        // sp_executesql-style wrapping rejects CREATE PROCEDURE (error 156)
        // and silently discards USE database-context changes between batches.
        // simple_query gives true SSMS batch semantics AND still streams rows,
        // so DECLARE…SELECT, IF…SELECT etc. work too.
        if is_dml_batch(sql) {
            let res = self.client.execute(sql, &[]).await?;
            let affected: u64 = res.rows_affected().iter().sum();
            let _ = tx.send(QueryEvent::Done { row_count: 0, affected_rows: affected }).await;
            return Ok(());
        }
        let mut stream = self.client.simple_query(sql).await?;
        let mut sent_cols = false;
        let mut chunk: Vec<Vec<serde_json::Value>> = vec![];
        let mut row_count = 0u64;
        while let Some(item) = stream.try_next().await? {
            match item {
                tiberius::QueryItem::Metadata(meta) => {
                    if !sent_cols {
                        let cols = meta.columns().iter()
                            .map(|c| ColumnMeta { name: c.name().to_string(), data_type: format!("{:?}", c.column_type()) })
                            .collect();
                        let _ = tx.send(QueryEvent::Columns(cols)).await;
                        sent_cols = true;
                    }
                }
                tiberius::QueryItem::Row(row) => {
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
        let _ = tx.send(QueryEvent::Done { row_count, affected_rows: 0 }).await;
        Ok(())
    }
}

#[cfg(test)]
mod live_tests {
    // Run explicitly against the local Docker SQL Server:
    //   cargo test --lib mssql::live_tests -- --ignored --nocapture --test-threads=1
    use super::*;
    use crate::changeset::Changeset;
    use crate::drivers::{Filter, FilterOp, FetchOptions, Sort};

    pub(super) async fn _drv() -> MssqlDriver { drv().await }
    async fn drv() -> MssqlDriver {
        MssqlDriver::connect("127.0.0.1", 1433, "sa", "YourStrongPassword123!", "tabula_mssql_test")
            .await.expect("connect")
    }
    fn t(name: &str) -> TableInfo {
        TableInfo { schema: Some("dbo".into()), name: name.into(), kind: TableKind::Table }
    }
    fn opts(filters: Vec<Filter>) -> FetchOptions {
        FetchOptions { limit: 100, offset: 0, sort: None, filters }
    }

    #[tokio::test]
    #[ignore]
    async fn reads() {
        let d = drv().await;
        let tables = d.list_tables().await.expect("list_tables");
        assert!(tables.iter().any(|t| t.name == "agents"), "agents missing: {tables:?}");
        assert!(tables.iter().any(|t| t.name == "calls"));

        assert_eq!(d.primary_key(&t("agents")).await.unwrap(), vec!["id".to_string()]);

        let page = d.fetch_rows(&t("agents"), &opts(vec![])).await.expect("fetch_rows");
        assert!(page.rows.len() >= 2, "rows: {:?}", page.rows);
        assert!(page.columns.iter().any(|c| c.name == "user_name"));
        println!("agents columns: {:?}", page.columns.iter().map(|c| (&c.name, &c.data_type)).collect::<Vec<_>>());
        println!("first row: {:?}", page.rows.first());

        let st = d.table_structure(&t("calls")).await.expect("structure");
        let fk = st.columns.iter().find(|c| c.name == "agent_id").unwrap();
        assert_eq!(fk.foreign_key.as_deref(), Some("agents(id)"), "fk: {:?}", fk.foreign_key);

        // sort + paging smoke
        let sorted = FetchOptions { limit: 1, offset: 0, sort: Some(Sort { column: "id".into(), desc: true }), filters: vec![] };
        let p2 = d.fetch_rows(&t("agents"), &sorted).await.unwrap();
        assert_eq!(p2.rows.len(), 1);
        println!("estimate: {:?}", page.total_estimate);
    }

    #[tokio::test]
    #[ignore]
    async fn write_round_trip() {
        let d = drv().await;
        let map = |v: serde_json::Value| v.as_object().unwrap().clone();

        // INSERT
        let insert = Changeset {
            updates: vec![], deletes: vec![],
            inserts: vec![map(serde_json::json!({"user_name": "ztest", "level": 5, "active": 1}))],
        };
        let r = d.apply_changeset(&t("agents"), &insert).await.expect("insert");
        assert_eq!(r.inserted, 1, "insert result: {r:?}");

        // find the new row's id
        let page = d.fetch_rows(&t("agents"), &opts(vec![
            Filter { column: "user_name".into(), op: FilterOp::Equals, value: "ztest".into() },
        ])).await.unwrap();
        let idx = page.columns.iter().position(|c| c.name == "id").unwrap();
        let id = page.rows[0][idx].clone();
        assert!(id.is_number(), "id: {id:?}");

        // UPDATE
        let mut pk = serde_json::Map::new();
        pk.insert("id".into(), id.clone());
        let update = Changeset {
            inserts: vec![], deletes: vec![],
            updates: vec![crate::changeset::RowUpdate {
                pk: pk.clone(),
                changes: vec![crate::changeset::CellChange {
                    column: "level".into(), old: serde_json::json!(5), new: serde_json::json!(9),
                }],
            }],
        };
        let r = d.apply_changeset(&t("agents"), &update).await.expect("update");
        assert_eq!(r.updated, 1, "update result: {r:?}");

        // DELETE (cleanup)
        let del = Changeset {
            inserts: vec![], updates: vec![],
            deletes: vec![crate::changeset::RowDelete { pk }],
        };
        let r = d.apply_changeset(&t("agents"), &del).await.expect("delete");
        assert_eq!(r.deleted, 1, "delete result: {r:?}");

        // verify gone
        let page = d.fetch_rows(&t("agents"), &opts(vec![
            Filter { column: "user_name".into(), op: FilterOp::Equals, value: "ztest".into() },
        ])).await.unwrap();
        assert_eq!(page.rows.len(), 0, "ztest should be deleted");
        println!("write round-trip OK");
    }
}

#[cfg(test)]
mod session_test {
    use super::*;
    use crate::drivers::QueryEvent;

    #[tokio::test]
    #[ignore]
    async fn session_stream_select_and_dml() {
        let d = super::live_tests::_drv().await;
        let mut sess = d.acquire_session().await.expect("session");
        assert!(sess.session_id().is_some(), "spid should be set");
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        sess.stream("SELECT id, user_name FROM dbo.agents ORDER BY id", tx).await.expect("stream");
        let mut cols = 0; let mut rows = 0; let mut done = false;
        while let Some(ev) = rx.recv().await {
            match ev {
                QueryEvent::Columns(c) => cols = c.len(),
                QueryEvent::Rows(r) => rows += r.len(),
                QueryEvent::Message(_) => {}
                QueryEvent::Done { row_count, .. } => { done = true; assert_eq!(row_count as usize, rows); }
            }
        }
        assert_eq!(cols, 2, "expected 2 columns");
        assert!(rows >= 2, "expected >=2 rows, got {rows}");
        assert!(done);
        println!("session SELECT: {cols} cols, {rows} rows");
    }
}

#[cfg(test)]
mod db_mgmt_test {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn create_list_drop() {
        let d = MssqlDriver::connect("127.0.0.1", 1433, "sa", "YourStrongPassword123!", "master")
            .await.expect("connect");
        let _ = d.drop_database("tabula_dbtest").await; // idempotent cleanup
        // create
        d.create_database("tabula_dbtest").await.expect("create");
        let dbs = d.list_databases().await.expect("list");
        assert!(dbs.iter().any(|x| x.name == "tabula_dbtest"), "created db not listed: {:?}", dbs.iter().map(|x| &x.name).collect::<Vec<_>>());
        println!("databases: {:?}", dbs.iter().map(|x| (&x.name, x.size_bytes)).collect::<Vec<_>>());
        // drop
        d.drop_database("tabula_dbtest").await.expect("drop");
        let dbs2 = d.list_databases().await.expect("list2");
        assert!(!dbs2.iter().any(|x| x.name == "tabula_dbtest"), "db not dropped");
        println!("create/list/drop OK");
    }
}

#[cfg(test)]
mod no_db_test {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn connect_without_database() {
        for host in ["127.0.0.1", "localhost"] {
            let t = std::time::Instant::now();
            match MssqlDriver::connect(host, 1433, "sa", "YourStrongPassword123!", "").await {
                Ok(d) => {
                    let dbs = d.list_databases().await.map(|v| v.len());
                    println!("{host}: OK in {:?}, list_databases -> {:?}", t.elapsed(), dbs);
                }
                Err(e) => println!("{host}: ERR in {:?}: {:?}", t.elapsed(), e),
            }
        }
    }
}

#[cfg(test)]
mod err_surface_test {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn bad_password_reports_login_failure_not_timeout() {
        let t = std::time::Instant::now();
        let err = MssqlDriver::connect("127.0.0.1", 1433, "sa", "WrongPassword!", "")
            .await.err().expect("must fail");
        println!("err in {:?}: {}", t.elapsed(), err.message);
        assert!(err.message.to_lowercase().contains("login"), "expected real login error, got: {}", err.message);
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "should fail fast, not wait out a pool timeout");
    }
}

#[cfg(test)]
mod proc_batch_test {
    use super::*;
    use crate::drivers::{QueryEvent, SqlDriver};

    async fn run_batch(sess: &mut Box<dyn crate::drivers::QuerySession>, sql: &str) -> Result<(), crate::error::AppError> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let r = sess.stream(sql, tx).await;
        while rx.recv().await.is_some() {}
        r
    }

    #[tokio::test]
    #[ignore]
    async fn create_procedure_batch_via_session() {
        let d = MssqlDriver::connect("127.0.0.1", 1433, "sa", "YourStrongPassword123!", "master").await.expect("connect");
        let mut s = d.acquire_session().await.expect("session");
        run_batch(&mut s, "IF DB_ID('tabula_proc_test') IS NULL CREATE DATABASE tabula_proc_test;").await.expect("createdb");
        run_batch(&mut s, "USE tabula_proc_test;").await.expect("use");
        // USE must persist to the NEXT batch (raw-batch semantics, like SSMS)
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        s.stream("SELECT DB_NAME() AS db;", tx).await.expect("dbname");
        let mut db = String::new();
        while let Some(ev) = rx.recv().await {
            if let QueryEvent::Rows(r) = ev {
                if let Some(serde_json::Value::String(n)) = r.first().and_then(|row| row.first()) { db = n.clone(); }
            }
        }
        assert_eq!(db, "tabula_proc_test", "USE did not persist across batches");
        let _ = run_batch(&mut s, "DROP PROCEDURE IF EXISTS dbo.TestProc;").await;
        // the exact shape the splitter emits: CREATE PROCEDURE first in batch, semicolons inside
        run_batch(&mut s, "CREATE PROCEDURE dbo.TestProc\nAS\nBEGIN\n    SET NOCOUNT ON;\n    SELECT 1 AS x;\nEND;").await.expect("create proc");
        // and EXEC streams rows
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        s.stream("EXEC dbo.TestProc;", tx).await.expect("exec");
        let mut rows = 0;
        while let Some(ev) = rx.recv().await { if let QueryEvent::Rows(r) = ev { rows += r.len(); } }
        assert_eq!(rows, 1, "EXEC should return 1 row");
        run_batch(&mut s, "USE master;").await.ok();
        drop(s);
        d.drop_database("tabula_proc_test").await.ok();
        println!("CREATE PROCEDURE + EXEC via session OK");
    }
}

#[cfg(test)]
mod print_msg_test {
    use super::*;
    use crate::drivers::QueryEvent;

    #[tokio::test]
    #[ignore]
    async fn print_output_is_captured_as_messages() {
        use tracing_subscriber::prelude::*;
        let _ = tracing_subscriber::registry().with(TiberiusMessageLayer).try_init();

        let d = MssqlDriver::connect("127.0.0.1", 1433, "sa", "YourStrongPassword123!", "master").await.expect("connect");
        let mut s = d.acquire_session().await.expect("session");
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        s.stream("DECLARE @i INT = 1;\nWHILE @i <= 3\nBEGIN\n  PRINT CONCAT('Iteration ', @i);\n  SET @i += 1;\nEND;", tx)
            .await.expect("stream");
        let mut msgs: Vec<String> = vec![];
        while let Some(ev) = rx.recv().await {
            if let QueryEvent::Message(m) = ev { msgs.push(m); }
        }
        println!("messages: {msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("Iteration 1")), "missing Iteration 1: {msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("Iteration 3")), "missing Iteration 3: {msgs:?}");
    }
}

#[cfg(test)]
mod router_tests {
    use super::is_dml_batch;

    #[test]
    fn plain_dml_and_selects() {
        assert!(is_dml_batch("UPDATE t SET x = 1"));
        assert!(is_dml_batch("  insert into t values (1)"));
        assert!(!is_dml_batch("SELECT * FROM t"));
        assert!(!is_dml_batch("CREATE PROCEDURE p AS BEGIN SELECT 1; END"));
        assert!(!is_dml_batch("EXEC dbo.p"));
    }

    #[test]
    fn comment_prefixed_dml_is_detected() {
        assert!(is_dml_batch("-- JIRA-123 fix\nUPDATE t SET x = 1"));
        assert!(is_dml_batch("/* audit\n note */ DELETE FROM t WHERE id = 1"));
        assert!(!is_dml_batch("-- note\nSELECT 1"));
    }

    #[test]
    fn cte_led_batches_route_by_final_verb() {
        assert!(is_dml_batch("WITH c AS (SELECT id FROM t WHERE s = ')') UPDATE c SET id = 1"));
        assert!(is_dml_batch("with a (x) as (select 1), b as (select 2) delete from t"));
        assert!(!is_dml_batch("WITH c AS (SELECT 1 AS x) SELECT * FROM c"));
    }
}
