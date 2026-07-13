use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellChange {
    pub column: String,
    pub old: serde_json::Value,
    pub new: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowUpdate {
    pub pk: serde_json::Map<String, serde_json::Value>,
    pub changes: Vec<CellChange>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDelete {
    pub pk: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changeset {
    pub updates: Vec<RowUpdate>,
    pub inserts: Vec<serde_json::Map<String, serde_json::Value>>,
    pub deletes: Vec<RowDelete>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatementKind { Update, Insert, Delete }

#[derive(Debug, Clone)]
pub struct RenderedStatement {
    pub sql: String,
    pub kind: StatementKind,
    /// true for updates/deletes: rows_affected must be exactly 1 (optimistic check)
    pub expect_one: bool,
    /// "id=3" — used in conflict error messages
    pub pk_desc: String,
}

pub fn escape_std(s: &str) -> String {
    s.replace('\'', "''")
}

pub fn escape_mysql(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "''")
}

pub fn literal(v: &serde_json::Value, escape: fn(&str) -> String) -> String {
    match v {
        serde_json::Value::Null => "NULL".into(),
        serde_json::Value::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", escape(s)),
        other => format!("'{}'", escape(&other.to_string())),
    }
}

fn pk_clause(
    pk: &serde_json::Map<String, serde_json::Value>,
    quote: fn(&str) -> String,
    escape: fn(&str) -> String,
) -> String {
    pk.iter()
        .map(|(k, v)| match v {
            serde_json::Value::Null => format!("{} IS NULL", quote(k)),
            _ => format!("{} = {}", quote(k), literal(v, escape)),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn pk_desc(pk: &serde_json::Map<String, serde_json::Value>) -> String {
    pk.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")
}

pub fn render_changeset(
    table: &crate::drivers::TableInfo,
    cs: &Changeset,
    quote: fn(&str) -> String,
    escape: fn(&str) -> String,
) -> Vec<RenderedStatement> {
    let name = crate::drivers::qualified_name(table, quote);
    let mut out = vec![];
    for u in &cs.updates {
        let sets = u.changes.iter()
            .map(|c| format!("{} = {}", quote(&c.column), literal(&c.new, escape)))
            .collect::<Vec<_>>()
            .join(", ");
        let olds = u.changes.iter()
            .map(|c| match &c.old {
                serde_json::Value::Null => format!("{} IS NULL", quote(&c.column)),
                v => format!("{} = {}", quote(&c.column), literal(v, escape)),
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        out.push(RenderedStatement {
            sql: format!("UPDATE {name} SET {sets} WHERE {} AND {olds}", pk_clause(&u.pk, quote, escape)),
            kind: StatementKind::Update,
            expect_one: true,
            pk_desc: pk_desc(&u.pk),
        });
    }
    for d in &cs.deletes {
        out.push(RenderedStatement {
            sql: format!("DELETE FROM {name} WHERE {}", pk_clause(&d.pk, quote, escape)),
            kind: StatementKind::Delete,
            expect_one: true,
            pk_desc: pk_desc(&d.pk),
        });
    }
    for ins in &cs.inserts {
        let cols = ins.keys().map(|k| quote(k)).collect::<Vec<_>>().join(", ");
        let vals = ins.values().map(|v| literal(v, escape)).collect::<Vec<_>>().join(", ");
        out.push(RenderedStatement {
            sql: format!("INSERT INTO {name} ({cols}) VALUES ({vals})"),
            kind: StatementKind::Insert,
            expect_one: false,
            pk_desc: String::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::{TableInfo, TableKind};

    fn q(s: &str) -> String { format!("\"{}\"", s) }
    fn t() -> TableInfo { TableInfo { schema: None, name: "people".into(), kind: TableKind::Table } }
    fn pk(v: i64) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("id".into(), serde_json::json!(v));
        m
    }

    #[test]
    fn literals() {
        assert_eq!(literal(&serde_json::json!(null), escape_std), "NULL");
        assert_eq!(literal(&serde_json::json!(5), escape_std), "5");
        assert_eq!(literal(&serde_json::json!(true), escape_std), "TRUE");
        assert_eq!(literal(&serde_json::json!("o'brien"), escape_std), "'o''brien'");
        assert_eq!(literal(&serde_json::json!("a\\b"), escape_mysql), "'a\\\\b'");
    }

    #[test]
    fn renders_update_with_optimistic_check() {
        let cs = Changeset {
            updates: vec![RowUpdate {
                pk: pk(3),
                changes: vec![
                    CellChange { column: "name".into(), old: serde_json::json!("Bob"), new: serde_json::json!("Rob") },
                    CellChange { column: "age".into(), old: serde_json::json!(null), new: serde_json::json!(30) },
                ],
            }],
            inserts: vec![],
            deletes: vec![],
        };
        let s = render_changeset(&t(), &cs, q, escape_std);
        assert_eq!(s.len(), 1);
        assert_eq!(
            s[0].sql,
            r#"UPDATE "people" SET "name" = 'Rob', "age" = 30 WHERE "id" = 3 AND "name" = 'Bob' AND "age" IS NULL"#
        );
        assert!(s[0].expect_one);
        assert_eq!(s[0].pk_desc, "id=3");
    }

    #[test]
    fn renders_delete_and_insert() {
        let mut ins = serde_json::Map::new();
        ins.insert("id".into(), serde_json::json!(9));
        ins.insert("name".into(), serde_json::json!("Zoe"));
        let cs = Changeset {
            updates: vec![],
            deletes: vec![RowDelete { pk: pk(2) }],
            inserts: vec![ins],
        };
        let s = render_changeset(&t(), &cs, q, escape_std);
        assert_eq!(s[0].sql, r#"DELETE FROM "people" WHERE "id" = 2"#);
        assert!(s[0].expect_one);
        assert_eq!(s[1].sql, r#"INSERT INTO "people" ("id", "name") VALUES (9, 'Zoe')"#);
        assert!(!s[1].expect_one);
    }
}
