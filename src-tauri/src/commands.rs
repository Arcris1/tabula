use crate::changeset::Changeset;
use crate::ddl::{render_ddl, DdlOp};
use crate::config::{ConnectionConfig, Engine};
use crate::credentials::CredentialStore;
use crate::drivers::keytree::{self, KeyTreeNode};
use crate::drivers::mysql::MysqlDriver;
use crate::drivers::postgres::PostgresDriver;
use crate::drivers::redis_kv::RedisDriver;
use crate::drivers::sqlite::SqliteDriver;
use crate::drivers::{AppliedResult, TableStructure, FetchOptions, KeyScanPage, QueryEvent, RedisEntry, RedisValueDto, RowsPage, TableColumns, TableInfo};
use crate::error::{AppError, ErrorKind};
use crate::history::HistoryEntry;
use crate::queries::RunningQuery;
use crate::registry::LiveConnection;
use crate::tunnel::{SshAuth, SshTunnel};
use crate::AppState;
use tauri::{Emitter, State};

async fn open_connection(
    cfg: &ConnectionConfig,
    password: Option<&str>,
    ssh_secret: Option<&str>,
    expected_host_key: Option<String>,
) -> Result<(LiveConnection, Option<SshTunnel>), AppError> {
    let user = cfg.username.as_deref().unwrap_or("");
    let db = cfg.database.as_deref().unwrap_or("");
    let pw = password.unwrap_or("");

    let default_port = match cfg.engine {
        Engine::Mysql => 3306,
        Engine::Postgres => 5432,
        Engine::Redis => 6379,
        Engine::Mssql => 1433,
        Engine::Sqlite => 0,
    };
    let mut host = cfg.host.clone().unwrap_or_else(|| "127.0.0.1".into());
    let mut port = cfg.port.unwrap_or(default_port);

    // spec: tunnel first, then pool through it
    let mut tunnel = None;
    if let Some(ssh) = &cfg.ssh {
        if !matches!(cfg.engine, Engine::Sqlite) {
            let t = SshTunnel::open(
                &ssh.host, ssh.port, &ssh.username,
                SshAuth { key_path: ssh.key_path.as_deref(), secret: ssh_secret },
                &host, port,
                expected_host_key,
            ).await?;
            host = "127.0.0.1".into();
            port = t.local_port;
            tunnel = Some(t);
        }
    }

    let conn = match cfg.engine {
        Engine::Sqlite => LiveConnection::Sql(Box::new(SqliteDriver::connect(db).await?)),
        Engine::Mysql => LiveConnection::Sql(Box::new(
            MysqlDriver::connect(&host, port, user, pw, db).await?,
        )),
        Engine::Postgres => LiveConnection::Sql(Box::new(
            PostgresDriver::connect(&host, port, user, pw, db).await?,
        )),
        Engine::Mssql => LiveConnection::Sql(Box::new(
            crate::drivers::mssql::MssqlDriver::connect(&host, port, user, pw, db).await?,
        )),
        Engine::Redis => LiveConnection::Kv(Box::new(
            RedisDriver::connect(&host, port, password, db.parse().unwrap_or(0)).await?,
        )),
    };
    Ok((conn, tunnel))
}

fn stored_or_given(id: &str, given: Option<String>) -> Result<Option<String>, AppError> {
    match given {
        Some(p) if !p.is_empty() => Ok(Some(p)),
        _ => CredentialStore::get(id),
    }
}

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, AppError> {
    state.store.lock().await.list()
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
    password: Option<String>,
    ssh_secret: Option<String>,
) -> Result<(), AppError> {
    if let Some(p) = &password {
        if !p.is_empty() {
            CredentialStore::set(&config.id, p)?;
        }
    }
    if let Some(p) = &ssh_secret {
        if !p.is_empty() {
            CredentialStore::set(&format!("{}.ssh", config.id), p)?;
        }
    }
    state.store.lock().await.upsert(config)
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    for s in state.sessions.drain_by_infix(&format!(":{id}:")).await {
        discard_session(s);
    }
    state.registry.remove_by_connection(&id).await; // pools (all windows)...
    state.tunnels.remove_by_connection(&id).await;  // ...then tunnels
    CredentialStore::delete(&id)?;
    CredentialStore::delete(&format!("{id}.ssh"))?;
    state.store.lock().await.delete(&id)
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
    password: Option<String>,
    ssh_secret: Option<String>,
) -> Result<(), AppError> {
    let pw = stored_or_given(&config.id, password)?;
    let ssh = match ssh_secret {
        Some(p) if !p.is_empty() => Some(p),
        _ => CredentialStore::get(&format!("{}.ssh", config.id))?,
    };
    let expected = match &config.ssh {
        Some(ssh) => state.known_hosts.lock().await.get(&ssh.host, ssh.port)?,
        None => None,
    };
    let (conn, tunnel) = open_connection(&config, pw.as_deref(), ssh.as_deref(), expected).await?;
    if let (Some(t), Some(ssh)) = (&tunnel, &config.ssh) {
        if let Some(fp) = &t.new_host_key {
            state.known_hosts.lock().await.put(&ssh.host, ssh.port, fp)?;
        }
    }
    match conn {
        LiveConnection::Sql(d) => d.ping().await,
        LiveConnection::Kv(d) => d.ping().await,
    }
}

#[tauri::command]
pub async fn connect(window: tauri::Window, state: State<'_, AppState>, id: String, database: Option<String>) -> Result<String, AppError> {
    let mut cfg = state.store.lock().await.list()?
        .into_iter().find(|c| c.id == id)
        .ok_or_else(|| AppError::internal(format!("unknown connection {id}")))?;
    // optional override: connect to a specific database (opening one from the stack view)
    if let Some(db) = database.filter(|d| !d.is_empty()) {
        cfg.database = Some(db);
    }
    let pw = CredentialStore::get(&id)?;
    let ssh_secret = CredentialStore::get(&format!("{id}.ssh"))?;
    let expected = match &cfg.ssh {
        Some(ssh) => state.known_hosts.lock().await.get(&ssh.host, ssh.port)?,
        None => None,
    };
    let (conn, tunnel) = open_connection(&cfg, pw.as_deref(), ssh_secret.as_deref(), expected).await?;
    if let (Some(t), Some(ssh)) = (&tunnel, &cfg.ssh) {
        if let Some(fp) = &t.new_host_key {
            state.known_hosts.lock().await.put(&ssh.host, ssh.port, fp)?;
        }
    }
    let paradigm = match &conn {
        LiveConnection::Sql(_) => "sql",
        LiveConnection::Kv(_) => "kv",
    };
    let key = scoped(&window, &id);
    state.registry.insert(&key, conn).await;
    if let Some(t) = tunnel {
        state.tunnels.insert(&key, t).await;
    }
    let _ = window.set_title(&format!("Tabula — {}", cfg.name));
    Ok(paradigm.to_string())
}

#[tauri::command]
pub async fn disconnect(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let key = scoped(&window, &id);
    for s in state.sessions.drain_by_prefix(&format!("{key}:")).await {
        discard_session(s); // sessions hold pooled connections: release first
    }
    state.registry.remove(&key).await; // then the pool...
    state.tunnels.remove(&key).await;  // ...then the tunnel
    let _ = window.set_title("Tabula");
    Ok(())
}

/// Rebuild a connection from scratch: drop its sessions, pool and SSH tunnel,
/// then connect again. Used to recover a connection whose network died (e.g.
/// laptop sleep / wifi off) without restarting the app — a dead SSH tunnel or
/// pool won't heal on its own, so we tear the whole thing down and reopen.
#[tauri::command]
pub async fn reconnect(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<String, AppError> {
    let key = scoped(&window, &id);
    for s in state.sessions.drain_by_prefix(&format!("{key}:")).await {
        discard_session(s);
    }
    state.registry.remove(&key).await;
    state.tunnels.remove(&key).await;
    connect(window, state, id, None).await
}

fn scoped(window: &tauri::Window, id: &str) -> String {
    format!("{}:{}", window.label(), id)
}

fn session_key(window: &tauri::Window, id: &str, tab_id: &str) -> String {
    format!("{}:{}:{}", window.label(), id, tab_id)
}

/// Best-effort ROLLBACK, then drop — never return a dirty connection to the pool.
pub(crate) fn discard_session(mut s: Box<dyn crate::drivers::QuerySession>) {
    tauri::async_runtime::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let _ = s.stream("ROLLBACK", tx).await;
        while rx.recv().await.is_some() {}
    });
}

async fn live(state: &State<'_, AppState>, key: &str) -> Result<std::sync::Arc<LiveConnection>, AppError> {
    state.registry.get(key).await
        .ok_or_else(|| AppError::connection("not connected"))
}

#[tauri::command]
pub async fn list_tables(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<Vec<TableInfo>, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.list_tables().await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn list_functions(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<Vec<String>, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.list_functions().await,
        LiveConnection::Kv(_) => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn function_definition(window: tauri::Window, state: State<'_, AppState>, id: String, name: String) -> Result<String, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.function_definition(&name).await,
        LiveConnection::Kv(_) => Ok(String::new()),
    }
}

#[tauri::command]
pub async fn export_table(window: tauri::Window, state: State<'_, AppState>, id: String, table: TableInfo, format: String, path: String) -> Result<u64, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => match format.as_str() {
            "csv" => crate::impexp::export_csv(d.as_ref(), &table, &path).await,
            "json" => crate::impexp::export_json(d.as_ref(), &table, &path).await,
            _ => Err(AppError::query("unsupported export format")),
        },
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn import_table(window: tauri::Window, state: State<'_, AppState>, id: String, table: TableInfo, format: String, path: String) -> Result<crate::impexp::ImportResult, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => match format.as_str() {
            "csv" => crate::impexp::import_csv(d.as_ref(), &table, &path).await,
            "json" => crate::impexp::import_json(d.as_ref(), &table, &path).await,
            "sql" => crate::impexp::import_sql(d.as_ref(), &path).await,
            _ => Err(AppError::query("unsupported import format")),
        },
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn fetch_rows(
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    table: TableInfo,
    opts: FetchOptions,
) -> Result<RowsPage, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.fetch_rows(&table, &opts).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn scan_keys(
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    pattern: String,
    cursor: u64,
) -> Result<KeyScanPage, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Kv(d) => d.scan_keys(&pattern, cursor, 200).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub async fn get_redis_value(
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    key: String,
) -> Result<RedisEntry, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Kv(d) => d.get_value(&key).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub async fn close_session(
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    tab_id: String,
) -> Result<(), AppError> {
    if let Some(s) = state.sessions.remove(&session_key(&window, &id, &tab_id)).await {
        discard_session(s);
    }
    Ok(())
}

#[tauri::command]
pub async fn forget_host_key(state: State<'_, AppState>, host: String, port: u16) -> Result<(), AppError> {
    state.known_hosts.lock().await.forget(&host, port)
}

#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<(), AppError> {
    std::fs::write(&path, contents)?;
    Ok(())
}

#[tauri::command]
pub fn build_key_tree(keys: Vec<String>) -> Vec<KeyTreeNode> {
    keytree::build(&keys)
}

#[tauri::command]
pub async fn open_new_window(app: tauri::AppHandle) -> Result<(), AppError> {
    let label = format!("win-{}", uuid::Uuid::new_v4().simple());
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::default())
        .title("Tabula")
        .inner_size(1200.0, 800.0)
        .build()
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn schema_columns(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<Vec<TableColumns>, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.columns_meta().await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn run_query(
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    sql: String,
    query_id: String, // frontend-generated so the run is registered before any event can fire
    tab_id: String,
) -> Result<String, AppError> {
    let key = scoped(&window, &id);
    let conn = live(&state, &key).await?;
    // per-tab persistent session: BEGIN/COMMIT/ROLLBACK span statements
    let skey = session_key(&window, &id, &tab_id);
    let mut session = match state.sessions.take(&skey).await {
        Some(s) => s,
        None => match &*conn {
            LiveConnection::Sql(d) => d.acquire_session().await?,
            LiveConnection::Kv(_) => return Err(AppError::query("not a SQL connection")),
        },
    };
    let session_id = session.session_id();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<QueryEvent>(8);

    // forwarder: channel -> window events
    let fwd_win = window.clone();
    let fwd_qid = query_id.clone();
    let started = std::time::Instant::now();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                QueryEvent::Columns(c) => {
                    let _ = fwd_win.emit("query:columns", serde_json::json!({"queryId": fwd_qid, "columns": c}));
                }
                QueryEvent::Rows(r) => {
                    let _ = fwd_win.emit("query:rows", serde_json::json!({"queryId": fwd_qid, "rows": r}));
                }
                QueryEvent::Message(m) => {
                    let _ = fwd_win.emit("query:message", serde_json::json!({"queryId": fwd_qid, "message": m}));
                }
                QueryEvent::Done { row_count, affected_rows } => {
                    let _ = fwd_win.emit("query:done", serde_json::json!({
                        "queryId": fwd_qid,
                        "rowCount": row_count,
                        "affectedRows": affected_rows,
                        "elapsedMs": started.elapsed().as_millis() as u64,
                    }));
                }
            }
        }
    });

    // executor
    let exec_win = window.clone();
    let exec_qid = query_id.clone();
    let exec_skey = skey.clone();
    tokio::spawn(async move {
        let result = session.stream(&sql, tx).await;
        // A dropped/idle-timed-out connection is unusable — discard it so the
        // tab's next run acquires a fresh session (auto-reconnect). Other errors
        // (SQL syntax, mid-transaction failures) keep the session ROLLBACK-able.
        let connection_dead = matches!(&result, Err(e) if e.kind == ErrorKind::ConnectionFailed);
        if let Err(e) = result {
            let _ = exec_win.emit("query:error", serde_json::json!({"queryId": exec_qid, "error": e}));
        }
        use tauri::Manager;
        let state = exec_win.app_handle().state::<crate::AppState>();
        if connection_dead {
            drop(session); // next run_query on this tab reconnects
        } else {
            state.sessions.put(&exec_skey, session).await;
        }
        state.queries.remove(&exec_qid).await;
    });

    state.queries.insert(&query_id, RunningQuery {
        session_id,
        connection_id: key, // window-scoped registry key
    }).await;
    Ok(query_id)
}

#[tauri::command]
pub async fn table_primary_key(window: tauri::Window, state: State<'_, AppState>, id: String, table: TableInfo) -> Result<Vec<String>, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.primary_key(&table).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn preview_changeset(window: tauri::Window, state: State<'_, AppState>, id: String, table: TableInfo, changeset: Changeset) -> Result<Vec<String>, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => Ok(d.preview_changeset(&table, &changeset)),
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn apply_changeset(window: tauri::Window, state: State<'_, AppState>, id: String, table: TableInfo, changeset: Changeset) -> Result<AppliedResult, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.apply_changeset(&table, &changeset).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn set_redis_value(window: tauri::Window, state: State<'_, AppState>, id: String, key: String, value: RedisValueDto) -> Result<(), AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Kv(d) => d.set_value(&key, &value).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub async fn delete_redis_key(window: tauri::Window, state: State<'_, AppState>, id: String, key: String) -> Result<(), AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Kv(d) => d.delete_key(&key).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub async fn set_redis_ttl(window: tauri::Window, state: State<'_, AppState>, id: String, key: String, ttl_secs: i64) -> Result<(), AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Kv(d) => d.set_ttl(&key, ttl_secs).await,
        LiveConnection::Sql(_) => Err(AppError::query("not a Redis connection")),
    }
}

#[tauri::command]
pub async fn table_structure(window: tauri::Window, state: State<'_, AppState>, id: String, table: TableInfo) -> Result<TableStructure, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.table_structure(&table).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn preview_ddl(window: tauri::Window, state: State<'_, AppState>, id: String, op: DdlOp) -> Result<String, AppError> {
    crate::ddl::validate_ddl(&op)?;
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => Ok(render_ddl(&op, d.ddl_flavor())),
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn apply_ddl(window: tauri::Window, state: State<'_, AppState>, id: String, op: DdlOp) -> Result<(), AppError> {
    crate::ddl::validate_ddl(&op)?; // reject smuggled SQL in the free-text type field
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.execute_raw(&render_ddl(&op, d.ddl_flavor())).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn list_history(state: State<'_, AppState>, connection_id: String) -> Result<Vec<HistoryEntry>, AppError> {
    state.history.lock().await.list(&connection_id)
}

#[tauri::command]
pub async fn add_history(state: State<'_, AppState>, entry: HistoryEntry) -> Result<(), AppError> {
    state.history.lock().await.add(entry)
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, query_id: String) -> Result<(), AppError> {
    if let Some(rq) = state.queries.remove(&query_id).await {
        if let Some(sid) = rq.session_id {
            if let Some(conn) = state.registry.get(&rq.connection_id).await {
                if let LiveConnection::Sql(d) = &*conn {
                    // server-side kill only: the stream errors out naturally, the
                    // executor returns the session, and the frontend always settles.
                    // Aborting the task here would drop the session mid-transaction
                    // and return a dirty connection to the pool.
                    let _ = d.kill_session(&sid).await;
                }
            }
        }
    }
    Ok(())
}

// ---- local-stack service management ----

#[tauri::command]
pub async fn list_services() -> Result<Vec<crate::services::ServiceInfo>, AppError> {
    tokio::task::spawn_blocking(crate::services::list)
        .await
        .map_err(|e| AppError::internal(e.to_string()))
}

#[tauri::command]
pub async fn service_action(id: String, action: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || crate::services::action(&id, &action))
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
}

// ---- database management (local-stack Databases view) ----

#[tauri::command]
pub async fn list_databases(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<Vec<crate::drivers::DbInfo>, AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.list_databases().await,
        LiveConnection::Kv(_) => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn create_database(window: tauri::Window, state: State<'_, AppState>, id: String, name: String) -> Result<(), AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.create_database(&name).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

#[tauri::command]
pub async fn drop_database(window: tauri::Window, state: State<'_, AppState>, id: String, name: String) -> Result<(), AppError> {
    match &*live(&state, &scoped(&window, &id)).await? {
        LiveConnection::Sql(d) => d.drop_database(&name).await,
        LiveConnection::Kv(_) => Err(AppError::query("not a SQL connection")),
    }
}

// ---- log tailing (local-stack Logs view) ----

#[tauri::command]
pub async fn list_logs() -> Result<Vec<crate::logs::LogSource>, AppError> {
    tokio::task::spawn_blocking(crate::logs::list)
        .await
        .map_err(|e| AppError::internal(e.to_string()))
}

#[tauri::command]
pub async fn read_log(path: String, from: Option<u64>) -> Result<crate::logs::LogChunk, AppError> {
    tokio::task::spawn_blocking(move || crate::logs::read(&path, from))
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
}

// ---- local-stack Tools / Mailpit ----

#[tauri::command]
pub async fn check_ports(ports: Vec<u16>) -> Result<Vec<crate::services::PortStatus>, AppError> {
    tokio::task::spawn_blocking(move || crate::services::check_ports(&ports))
        .await
        .map_err(|e| AppError::internal(e.to_string()))
}

/// Open a URL or filesystem path with the OS default handler (browser/Finder).
#[tauri::command]
pub async fn open_external(target: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    let (cmd, args): (&str, Vec<&str>) = ("open", vec![target.as_str()]);
    #[cfg(target_os = "windows")]
    let (cmd, args): (&str, Vec<&str>) = ("cmd", vec!["/C", "start", "", target.as_str()]);
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let (cmd, args): (&str, Vec<&str>) = ("xdg-open", vec![target.as_str()]);

    let status = std::process::Command::new(cmd).args(&args).status()?;
    if !status.success() {
        return Err(AppError::internal(format!("failed to open {target}")));
    }
    Ok(())
}
