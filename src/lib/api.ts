import { invoke } from "@tauri-apps/api/core";

/** Error text that indicates the server dropped an idle connection. */
function isConnectionLost(e: unknown): boolean {
  const msg = (e instanceof Error ? e.message : String(e)).toLowerCase();
  return (
    msg.includes("connection") &&
    (msg.includes("closed") || msg.includes("reset") || msg.includes("broken") ||
      msg.includes("lost") || msg.includes("terminat") || msg.includes("gone away") ||
      msg.includes("unexpectedly"))
  );
}

/**
 * Run an operation, retrying once if the connection was dropped while idle.
 * The pool opens a fresh connection on the second acquire, so a transient
 * idle-timeout disconnect recovers without the user noticing.
 */
async function withReconnect<T>(op: () => Promise<T>): Promise<T> {
  try {
    return await op();
  } catch (e) {
    if (!isConnectionLost(e)) throw e;
    return await op();
  }
}

export type Engine = "mysql" | "postgres" | "sqlite" | "redis" | "mssql";
export type EnvColor = "gray" | "blue" | "green" | "amber" | "red";

export interface SshConfig {
  host: string;
  port: number;
  username: string;
  keyPath?: string | null;
}

export interface ConnectionConfig {
  id: string;
  name: string;
  engine: Engine;
  color: EnvColor;
  host?: string | null;
  port?: number | null;
  username?: string | null;
  database?: string | null;
  isProduction: boolean;
  ssh?: SshConfig | null;
}

export interface AppError {
  kind: string;
  message: string;
  detail?: string | null;
}

export interface TableInfo {
  schema?: string | null;
  name: string;
  kind: "table" | "view";
}
export interface ColumnMeta {
  name: string;
  dataType: string;
}
export interface RowsPage {
  columns: ColumnMeta[];
  rows: unknown[][];
  totalEstimate?: number | null;
  query: string;
}
export type FilterOp = "contains" | "equals" | "isNull" | "notNull";
export interface Filter {
  column: string;
  op: FilterOp;
  value: string;
}
export interface Sort {
  column: string;
  desc: boolean;
}
export interface FetchOptions {
  limit: number;
  offset: number;
  sort?: Sort | null;
  filters: Filter[];
}
export interface KeyScanPage {
  keys: string[];
  cursor: number;
}
export interface KeyTreeNode {
  name: string;
  fullKey?: string | null;
  children: KeyTreeNode[];
  count: number;
}
export interface TableColumns {
  schema?: string | null;
  table: string;
  columns: string[];
}
export interface HistoryEntry {
  connectionId: string;
  sql: string;
  startedAtMs: number;
  durationMs: number;
  status: "ok" | "error";
  rowCount?: number | null;
  error?: string | null;
}
export interface StructColumn { name: string; dataType: string; nullable: boolean; default?: string | null; isPk: boolean; isComputed: boolean; foreignKey?: string | null; comment?: string | null }
export interface StructIndex { name: string; columns: string[]; unique: boolean }
export interface StructTrigger { name: string; timing: string; event: string }
export interface TableStructure { columns: StructColumn[]; indexes: StructIndex[]; triggers: StructTrigger[]; comment?: string | null }
export interface ColumnDef { name: string; dataType: string; nullable: boolean; isPk: boolean }
export type DdlOp =
  | { op: "createTable"; name: string; columns: ColumnDef[] }
  | { op: "addColumn"; table: TableInfo; column: ColumnDef }
  | { op: "dropColumn"; table: TableInfo; column: string }
  | { op: "addIndex"; table: TableInfo; name: string; columns: string[]; unique: boolean }
  | { op: "dropIndex"; table: TableInfo; name: string }
  | { op: "dropTable"; table: TableInfo }
  | { op: "truncateTable"; table: TableInfo };
export interface CellChange { column: string; old: unknown; new: unknown }
export interface RowUpdate { pk: Record<string, unknown>; changes: CellChange[] }
export interface RowDelete { pk: Record<string, unknown> }
export interface Changeset { updates: RowUpdate[]; inserts: Record<string, unknown>[]; deletes: RowDelete[] }
export interface AppliedResult { updated: number; inserted: number; deleted: number }
export type RedisValue =
  | { type: "string"; value: string }
  | { type: "hash"; fields: [string, string][] }
  | { type: "list"; items: string[] }
  | { type: "set"; members: string[] }
  | { type: "zSet"; members: [string, number][] };
export interface RedisEntry {
  key: string;
  ttlSecs: number;
  value: RedisValue;
  /** full server-side length for capped types (list/zset); null/undefined = complete */
  totalItems?: number | null;
}

export interface DbInfo {
  name: string;
  sizeBytes?: number | null;
}

export const api = {
  listConnections: () => invoke<ConnectionConfig[]>("list_connections"),
  saveConnection: (config: ConnectionConfig, password?: string, sshSecret?: string) =>
    invoke<void>("save_connection", { config, password: password ?? null, sshSecret: sshSecret ?? null }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),
  testConnection: (config: ConnectionConfig, password?: string, sshSecret?: string) =>
    invoke<void>("test_connection", { config, password: password ?? null, sshSecret: sshSecret ?? null }),
  connect: (id: string, database?: string) => invoke<"sql" | "kv">("connect", { id, database: database ?? null }),
  reconnect: (id: string) => invoke<"sql" | "kv">("reconnect", { id }),
  disconnect: (id: string) => invoke<void>("disconnect", { id }),
  listTables: (id: string) => invoke<TableInfo[]>("list_tables", { id }),
  listFunctions: (id: string) => invoke<string[]>("list_functions", { id }),
  functionDefinition: (id: string, name: string) => invoke<string>("function_definition", { id, name }),
  exportTable: (id: string, table: TableInfo, format: "csv" | "json", path: string) =>
    invoke<number>("export_table", { id, table, format, path }),
  importTable: (id: string, table: TableInfo, format: "csv" | "json" | "sql", path: string) =>
    invoke<{ inserted: number }>("import_table", { id, table, format, path }),
  fetchRows: (id: string, table: TableInfo, opts: FetchOptions) =>
    withReconnect(() => invoke<RowsPage>("fetch_rows", { id, table, opts })),
  scanKeys: (id: string, pattern: string, cursor: number) =>
    invoke<KeyScanPage>("scan_keys", { id, pattern, cursor }),
  getRedisValue: (id: string, key: string) => invoke<RedisEntry>("get_redis_value", { id, key }),
  buildKeyTree: (keys: string[]) => invoke<KeyTreeNode[]>("build_key_tree", { keys }),
  writeTextFile: (path: string, contents: string) => invoke<void>("write_text_file", { path, contents }),
  runQuery: (id: string, sql: string, queryId: string, tabId: string) =>
    invoke<string>("run_query", { id, sql, queryId, tabId }),
  closeSession: (id: string, tabId: string) => invoke<void>("close_session", { id, tabId }),
  cancelQuery: (queryId: string) => invoke<void>("cancel_query", { queryId }),
  schemaColumns: (id: string) => invoke<TableColumns[]>("schema_columns", { id }),
  listHistory: (connectionId: string) => invoke<HistoryEntry[]>("list_history", { connectionId }),
  addHistory: (entry: HistoryEntry) => invoke<void>("add_history", { entry }),
  tablePrimaryKey: (id: string, table: TableInfo) =>
    invoke<string[]>("table_primary_key", { id, table }),
  previewChangeset: (id: string, table: TableInfo, changeset: Changeset) =>
    invoke<string[]>("preview_changeset", { id, table, changeset }),
  applyChangeset: (id: string, table: TableInfo, changeset: Changeset) =>
    invoke<AppliedResult>("apply_changeset", { id, table, changeset }),
  setRedisValue: (id: string, key: string, value: RedisValue) =>
    invoke<void>("set_redis_value", { id, key, value }),
  deleteRedisKey: (id: string, key: string) => invoke<void>("delete_redis_key", { id, key }),
  setRedisTtl: (id: string, key: string, ttlSecs: number) =>
    invoke<void>("set_redis_ttl", { id, key, ttlSecs }),
  tableStructure: (id: string, table: TableInfo) =>
    invoke<TableStructure>("table_structure", { id, table }),
  previewDdl: (id: string, op: DdlOp) => invoke<string>("preview_ddl", { id, op }),
  applyDdl: (id: string, op: DdlOp) => invoke<void>("apply_ddl", { id, op }),
  openNewWindow: () => invoke<void>("open_new_window"),
  forgetHostKey: (host: string, port: number) => invoke<void>("forget_host_key", { host, port }),
  listDatabases: (id: string) => invoke<DbInfo[]>("list_databases", { id }),
  createDatabase: (id: string, name: string) => invoke<void>("create_database", { id, name }),
  dropDatabase: (id: string, name: string) => invoke<void>("drop_database", { id, name }),
};
