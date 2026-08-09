use crate::drivers::{qualified_name, TableInfo};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor { Mysql, Postgres, Sqlite, Mssql }

fn quote_mysql(s: &str) -> String { format!("`{}`", s.replace('`', "``")) }
fn quote_std(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
fn quote_mssql(s: &str) -> String { format!("[{}]", s.replace(']', "]]")) }

impl Flavor {
    fn quote(self) -> fn(&str) -> String {
        match self {
            Flavor::Mysql => quote_mysql,
            Flavor::Mssql => quote_mssql,
            _ => quote_std,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_pk: bool,
    /// Optional DEFAULT expression (free-text, validated by `valid_default`).
    /// A default is what lets a NOT NULL column be added to a populated table.
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum DdlOp {
    CreateTable { name: String, columns: Vec<ColumnDef> },
    AddColumn { table: TableInfo, column: ColumnDef },
    DropColumn { table: TableInfo, column: String },
    /// Change a column's type and/or nullability. (Not supported on SQLite.)
    AlterColumn { table: TableInfo, column: String, data_type: String, nullable: bool },
    RenameColumn { table: TableInfo, column: String, new_name: String },
    RenameTable { table: TableInfo, new_name: String },
    AddIndex { table: TableInfo, name: String, columns: Vec<String>, unique: bool },
    DropIndex { table: TableInfo, name: String },
    DropTable { table: TableInfo },
    TruncateTable { table: TableInfo },
}

/// A column type is free-text and interpolated raw into DDL; restrict it to the
/// shape of a real type (`varchar(255)`, `numeric(10,2)`, `timestamp with time zone`,
/// `int[]`, MySQL `enum('a','b')`) while making it impossible to smuggle a second
/// column/constraint or statement past the preview. Beyond the char allowlist we
/// require parentheses to be balanced and never go negative, and commas to appear
/// only inside parens — that closes the `INT DEFAULT 0), hacked TEXT` break-out.
pub fn valid_data_type(t: &str) -> bool {
    let t = t.trim();
    if t.is_empty() || t.len() > 64 { return false; }
    if t.contains("--") { return false; }
    let mut depth: i32 = 0;
    for c in t.chars() {
        if !(c.is_ascii_alphanumeric() || " (),[]'".contains(c)) { return false; }
        match c {
            '(' => depth += 1,
            ')' => { depth -= 1; if depth < 0 { return false; } }
            ',' if depth == 0 => return false, // a top-level comma = a smuggled column
            _ => {}
        }
    }
    depth == 0
}

/// A DEFAULT expression is also interpolated raw. Allow numbers, quoted strings,
/// and simple keyword/function defaults (`CURRENT_TIMESTAMP`, `now()`, `-1`,
/// `'active'`) while blocking statement break-out (`;`, `--`) and unbalanced parens.
pub fn valid_default(d: &str) -> bool {
    let d = d.trim();
    if d.is_empty() || d.len() > 128 { return false; }
    if d.contains(';') || d.contains("--") { return false; }
    let mut depth: i32 = 0;
    for c in d.chars() {
        if !(c.is_ascii_alphanumeric() || " ()'.,:+-_".contains(c)) { return false; }
        match c {
            '(' => depth += 1,
            ')' => { depth -= 1; if depth < 0 { return false; } }
            _ => {}
        }
    }
    depth == 0
}

/// Escapes a value for use inside a T-SQL single-quoted string literal.
fn esc_lit(s: &str) -> String { s.replace('\'', "''") }

/// The `schema.table` target string sp_rename expects (unquoted names, quote-escaped).
fn sp_rename_target(table: &TableInfo) -> String {
    match &table.schema {
        Some(schema) => format!("{}.{}", esc_lit(schema), esc_lit(&table.name)),
        None => esc_lit(&table.name),
    }
}

fn column_sql(c: &ColumnDef, quote: fn(&str) -> String) -> String {
    let mut s = format!("{} {}", quote(&c.name), c.data_type);
    if !c.nullable {
        s.push_str(" NOT NULL");
    }
    if let Some(d) = c.default.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        s.push_str(&format!(" DEFAULT {d}"));
    }
    s
}

/// Some ops aren't expressible on every engine (e.g. SQLite can't ALTER a column
/// type in place). Returns a human message when `op` can't run on `flavor`.
pub fn unsupported_reason(op: &DdlOp, flavor: Flavor) -> Option<String> {
    match op {
        DdlOp::AlterColumn { .. } if flavor == Flavor::Sqlite => {
            Some("SQLite cannot change a column's type or nullability in place — recreate the table instead.".into())
        }
        _ => None,
    }
}

/// Validates every free-text fragment (column types, defaults) in an op before it
/// is rendered/executed.
pub fn validate_ddl(op: &DdlOp) -> Result<(), crate::error::AppError> {
    let bad_type = |t: &str| crate::error::AppError::query(format!("invalid column type: {t:?}"));
    let bad_def = |d: &str| crate::error::AppError::query(format!("invalid default value: {d:?}"));
    let check_col = |c: &ColumnDef| -> Result<(), crate::error::AppError> {
        if !valid_data_type(&c.data_type) { return Err(bad_type(&c.data_type)); }
        if let Some(d) = &c.default {
            if !valid_default(d) { return Err(bad_def(d)); }
        }
        Ok(())
    };
    match op {
        DdlOp::CreateTable { columns, .. } => {
            for c in columns { check_col(c)?; }
        }
        DdlOp::AddColumn { column, .. } => check_col(column)?,
        DdlOp::AlterColumn { data_type, .. } => {
            if !valid_data_type(data_type) { return Err(bad_type(data_type)); }
        }
        _ => {}
    }
    Ok(())
}

pub fn render_ddl(op: &DdlOp, flavor: Flavor) -> String {
    let quote = flavor.quote();
    match op {
        DdlOp::CreateTable { name, columns } => {
            let mut parts: Vec<String> = columns.iter().map(|c| column_sql(c, quote)).collect();
            let pk: Vec<String> = columns.iter().filter(|c| c.is_pk).map(|c| quote(&c.name)).collect();
            if !pk.is_empty() {
                parts.push(format!("PRIMARY KEY ({})", pk.join(", ")));
            }
            format!("CREATE TABLE {} ({})", quote(name), parts.join(", "))
        }
        DdlOp::AddColumn { table, column } => {
            // T-SQL uses `ADD` (no COLUMN keyword); others accept `ADD COLUMN`
            let add = if flavor == Flavor::Mssql { "ADD" } else { "ADD COLUMN" };
            format!("ALTER TABLE {} {} {}", qualified_name(table, quote), add, column_sql(column, quote))
        }
        DdlOp::DropColumn { table, column } => {
            format!("ALTER TABLE {} DROP COLUMN {}", qualified_name(table, quote), quote(column))
        }
        DdlOp::AlterColumn { table, column, data_type, nullable } => {
            let tbl = qualified_name(table, quote);
            let col = quote(column);
            let null = if *nullable { "" } else { " NOT NULL" };
            match flavor {
                // pg separates type and nullability into two ALTER COLUMN clauses
                Flavor::Postgres => format!(
                    "ALTER TABLE {tbl} ALTER COLUMN {col} TYPE {data_type}, ALTER COLUMN {col} {} NOT NULL",
                    if *nullable { "DROP" } else { "SET" },
                ),
                Flavor::Mysql => format!("ALTER TABLE {tbl} MODIFY COLUMN {col} {data_type}{null}"),
                // sqlite is rejected earlier by unsupported_reason
                _ => format!("ALTER TABLE {tbl} ALTER COLUMN {col} {data_type}{null}"),
            }
        }
        DdlOp::RenameColumn { table, column, new_name } => match flavor {
            // T-SQL renames via sp_rename with a 'schema.table.column' string
            Flavor::Mssql => format!(
                "EXEC sp_rename '{}.{}', '{}', 'COLUMN'",
                sp_rename_target(table), esc_lit(column), esc_lit(new_name),
            ),
            _ => format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                qualified_name(table, quote), quote(column), quote(new_name),
            ),
        },
        DdlOp::RenameTable { table, new_name } => match flavor {
            // sp_rename's new name must be UNqualified (it stays in the same schema)
            Flavor::Mssql => format!("EXEC sp_rename '{}', '{}'", sp_rename_target(table), esc_lit(new_name)),
            _ => format!("ALTER TABLE {} RENAME TO {}", qualified_name(table, quote), quote(new_name)),
        },
        DdlOp::AddIndex { table, name, columns, unique } => {
            format!(
                "CREATE {}INDEX {} ON {} ({})",
                if *unique { "UNIQUE " } else { "" },
                quote(name),
                qualified_name(table, quote),
                columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ")
            )
        }
        DdlOp::DropTable { table } => format!("DROP TABLE {}", qualified_name(table, quote)),
        DdlOp::TruncateTable { table } => match flavor {
            // sqlite has no TRUNCATE; DELETE is the equivalent
            Flavor::Sqlite => format!("DELETE FROM {}", qualified_name(table, quote)),
            _ => format!("TRUNCATE TABLE {}", qualified_name(table, quote)),
        },
        DdlOp::DropIndex { table, name } => match flavor {
            Flavor::Mysql | Flavor::Mssql => format!("DROP INDEX {} ON {}", quote(name), qualified_name(table, quote)),
            // postgres: indexes live in the table's schema and must be qualified
            _ => match &table.schema {
                Some(schema) => format!("DROP INDEX {}.{}", quote(schema), quote(name)),
                None => format!("DROP INDEX {}", quote(name)),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::TableKind;

    fn t() -> TableInfo { TableInfo { schema: None, name: "orders".into(), kind: TableKind::Table } }
    fn col(n: &str, ty: &str, nullable: bool, pk: bool) -> ColumnDef {
        ColumnDef { name: n.into(), data_type: ty.into(), nullable, is_pk: pk, default: None }
    }

    #[test]
    fn create_table_with_pk() {
        let op = DdlOp::CreateTable { name: "orders".into(), columns: vec![
            col("id", "INTEGER", false, true),
            col("note", "TEXT", true, false),
        ]};
        assert_eq!(render_ddl(&op, Flavor::Postgres),
            r#"CREATE TABLE "orders" ("id" INTEGER NOT NULL, "note" TEXT, PRIMARY KEY ("id"))"#);
        assert_eq!(render_ddl(&op, Flavor::Mysql),
            "CREATE TABLE `orders` (`id` INTEGER NOT NULL, `note` TEXT, PRIMARY KEY (`id`))");
    }

    #[test]
    fn add_drop_column() {
        let add = DdlOp::AddColumn { table: t(), column: col("qty", "INT", false, false) };
        assert_eq!(render_ddl(&add, Flavor::Mysql), "ALTER TABLE `orders` ADD COLUMN `qty` INT NOT NULL");
        let drop = DdlOp::DropColumn { table: t(), column: "qty".into() };
        assert_eq!(render_ddl(&drop, Flavor::Sqlite), r#"ALTER TABLE "orders" DROP COLUMN "qty""#);
    }

    #[test]
    fn drop_and_truncate_table() {
        let drop = DdlOp::DropTable { table: t() };
        assert_eq!(render_ddl(&drop, Flavor::Postgres), r#"DROP TABLE "orders""#);
        assert_eq!(render_ddl(&drop, Flavor::Mysql), "DROP TABLE `orders`");
        let trunc = DdlOp::TruncateTable { table: t() };
        assert_eq!(render_ddl(&trunc, Flavor::Postgres), r#"TRUNCATE TABLE "orders""#);
        // sqlite has no TRUNCATE
        assert_eq!(render_ddl(&trunc, Flavor::Sqlite), r#"DELETE FROM "orders""#);
    }

    #[test]
    fn add_drop_index_flavors() {
        let add = DdlOp::AddIndex { table: t(), name: "idx_qty".into(), columns: vec!["qty".into(), "id".into()], unique: true };
        assert_eq!(render_ddl(&add, Flavor::Postgres), r#"CREATE UNIQUE INDEX "idx_qty" ON "orders" ("qty", "id")"#);
        let drop = DdlOp::DropIndex { table: t(), name: "idx_qty".into() };
        assert_eq!(render_ddl(&drop, Flavor::Mysql), "DROP INDEX `idx_qty` ON `orders`");
        assert_eq!(render_ddl(&drop, Flavor::Postgres), r#"DROP INDEX "idx_qty""#);

        // regression (db-app-explorer round 2 B8): non-public schema must qualify
        let schema_table = TableInfo { schema: Some("app".into()), name: "orders".into(), kind: TableKind::Table };
        let drop = DdlOp::DropIndex { table: schema_table, name: "idx_qty".into() };
        assert_eq!(render_ddl(&drop, Flavor::Postgres), r#"DROP INDEX "app"."idx_qty""#);
    }

    // ---- Fix 4: alter/rename + defaults (persona round-3 findings) ----

    #[test]
    fn column_default_is_emitted() {
        let add = DdlOp::AddColumn {
            table: t(),
            column: ColumnDef { name: "status".into(), data_type: "INT".into(), nullable: false, is_pk: false, default: Some("0".into()) },
        };
        // a default lets a NOT NULL column be added to a populated table (esp. MSSQL)
        assert_eq!(render_ddl(&add, Flavor::Mssql), "ALTER TABLE [orders] ADD [status] INT NOT NULL DEFAULT 0");
        assert_eq!(render_ddl(&add, Flavor::Mysql), "ALTER TABLE `orders` ADD COLUMN `status` INT NOT NULL DEFAULT 0");
    }

    #[test]
    fn alter_column_type_and_nullability_per_flavor() {
        let op = DdlOp::AlterColumn { table: t(), column: "qty".into(), data_type: "BIGINT".into(), nullable: false };
        assert_eq!(render_ddl(&op, Flavor::Mysql), "ALTER TABLE `orders` MODIFY COLUMN `qty` BIGINT NOT NULL");
        assert_eq!(render_ddl(&op, Flavor::Mssql), "ALTER TABLE [orders] ALTER COLUMN [qty] BIGINT NOT NULL");
        assert_eq!(render_ddl(&op, Flavor::Postgres),
            r#"ALTER TABLE "orders" ALTER COLUMN "qty" TYPE BIGINT, ALTER COLUMN "qty" SET NOT NULL"#);
        let nullable = DdlOp::AlterColumn { table: t(), column: "qty".into(), data_type: "BIGINT".into(), nullable: true };
        assert_eq!(render_ddl(&nullable, Flavor::Postgres),
            r#"ALTER TABLE "orders" ALTER COLUMN "qty" TYPE BIGINT, ALTER COLUMN "qty" DROP NOT NULL"#);
        // SQLite can't do this in place
        assert!(unsupported_reason(&op, Flavor::Sqlite).is_some());
        assert!(unsupported_reason(&op, Flavor::Mysql).is_none());
    }

    #[test]
    fn rename_column_and_table_per_flavor() {
        let rc = DdlOp::RenameColumn { table: t(), column: "qty".into(), new_name: "quantity".into() };
        assert_eq!(render_ddl(&rc, Flavor::Postgres), r#"ALTER TABLE "orders" RENAME COLUMN "qty" TO "quantity""#);
        assert_eq!(render_ddl(&rc, Flavor::Mssql), "EXEC sp_rename 'orders.qty', 'quantity', 'COLUMN'");
        let rt = DdlOp::RenameTable { table: t(), new_name: "purchases".into() };
        assert_eq!(render_ddl(&rt, Flavor::Mysql), "ALTER TABLE `orders` RENAME TO `purchases`");
        assert_eq!(render_ddl(&rt, Flavor::Mssql), "EXEC sp_rename 'orders', 'purchases'");
        // MSSQL sp_rename target carries the schema, new name stays bare
        let schema_table = TableInfo { schema: Some("dbo".into()), name: "orders".into(), kind: TableKind::Table };
        let rt2 = DdlOp::RenameTable { table: schema_table, new_name: "purchases".into() };
        assert_eq!(render_ddl(&rt2, Flavor::Mssql), "EXEC sp_rename 'dbo.orders', 'purchases'");
    }

    #[test]
    fn data_type_validator_blocks_smuggled_columns_but_allows_enum() {
        assert!(valid_data_type("varchar(255)"));
        assert!(valid_data_type("numeric(10,2)"));
        assert!(valid_data_type("enum('a','b')")); // MySQL enum now allowed
        assert!(valid_data_type("timestamp with time zone"));
        assert!(!valid_data_type("INT DEFAULT 0), hacked TEXT")); // break-out closed
        assert!(!valid_data_type("INT, hacked TEXT"));            // top-level comma
        assert!(!valid_data_type("INT -- x"));
    }

    #[test]
    fn default_validator_blocks_statement_breakout() {
        assert!(valid_default("0"));
        assert!(valid_default("'active'"));
        assert!(valid_default("CURRENT_TIMESTAMP"));
        assert!(valid_default("now()"));
        assert!(valid_default("-1"));
        assert!(!valid_default("0); DROP TABLE users; --"));
        assert!(!valid_default("0 -- x"));
    }
}
