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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum DdlOp {
    CreateTable { name: String, columns: Vec<ColumnDef> },
    AddColumn { table: TableInfo, column: ColumnDef },
    DropColumn { table: TableInfo, column: String },
    AddIndex { table: TableInfo, name: String, columns: Vec<String>, unique: bool },
    DropIndex { table: TableInfo, name: String },
    DropTable { table: TableInfo },
    TruncateTable { table: TableInfo },
}

/// A column type is free-text and interpolated raw into DDL; restrict it to the
/// shape of a real type (`varchar(255)`, `numeric(10,2)`, `timestamp with time zone`,
/// `int[]`) so it can't smuggle arbitrary SQL past the preview.
pub fn valid_data_type(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty()
        && t.len() <= 64
        && t.chars().all(|c| c.is_ascii_alphanumeric() || " (),[]".contains(c))
}

fn column_sql(c: &ColumnDef, quote: fn(&str) -> String) -> String {
    let mut s = format!("{} {}", quote(&c.name), c.data_type);
    if !c.nullable {
        s.push_str(" NOT NULL");
    }
    s
}

/// Validates every column type in an op before it is rendered/executed.
pub fn validate_ddl(op: &DdlOp) -> Result<(), crate::error::AppError> {
    let bad = |t: &str| crate::error::AppError::query(format!("invalid column type: {t:?}"));
    match op {
        DdlOp::CreateTable { columns, .. } => {
            for c in columns {
                if !valid_data_type(&c.data_type) {
                    return Err(bad(&c.data_type));
                }
            }
        }
        DdlOp::AddColumn { column, .. } => {
            if !valid_data_type(&column.data_type) {
                return Err(bad(&column.data_type));
            }
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
        ColumnDef { name: n.into(), data_type: ty.into(), nullable, is_pk: pk }
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
}
