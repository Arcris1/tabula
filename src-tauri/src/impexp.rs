use crate::changeset::{Changeset, RowUpdate};
use crate::drivers::{Filter, FetchOptions, SqlDriver, Sort, TableInfo};
use crate::error::AppError;
use serde::Serialize;

/// serde_json::Value -> a flat cell string for CSV
fn cell_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Fetches every row matching the given sort+filters, paged (browse SELECT).
/// `sort`/`filters` come from the grid so an export reflects exactly what the
/// user is looking at — not the whole unfiltered table.
async fn all_rows(
    d: &dyn SqlDriver,
    table: &TableInfo,
    sort: Option<Sort>,
    filters: Vec<Filter>,
) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>), AppError> {
    const PAGE: i64 = 1000;
    let mut offset = 0;
    let mut cols: Vec<String> = vec![];
    let mut rows: Vec<Vec<serde_json::Value>> = vec![];
    loop {
        let p = d
            .fetch_rows(table, &FetchOptions { limit: PAGE, offset, sort: sort.clone(), filters: filters.clone() })
            .await?;
        if offset == 0 {
            cols = p.columns.iter().map(|c| c.name.clone()).collect();
        }
        let n = p.rows.len() as i64;
        rows.extend(p.rows);
        if n < PAGE { break; }
        offset += PAGE;
    }
    Ok((cols, rows))
}

pub async fn export_csv(
    d: &dyn SqlDriver,
    table: &TableInfo,
    path: &str,
    sort: Option<Sort>,
    filters: Vec<Filter>,
) -> Result<u64, AppError> {
    let (cols, rows) = all_rows(d, table, sort, filters).await?;
    let mut w = csv::Writer::from_path(path).map_err(|e| AppError::internal(e.to_string()))?;
    w.write_record(&cols).map_err(|e| AppError::internal(e.to_string()))?;
    for r in &rows {
        let record: Vec<String> = r.iter().map(cell_str).collect();
        w.write_record(&record).map_err(|e| AppError::internal(e.to_string()))?;
    }
    w.flush().map_err(|e| AppError::internal(e.to_string()))?;
    Ok(rows.len() as u64)
}

pub async fn export_json(
    d: &dyn SqlDriver,
    table: &TableInfo,
    path: &str,
    sort: Option<Sort>,
    filters: Vec<Filter>,
) -> Result<u64, AppError> {
    let (cols, rows) = all_rows(d, table, sort, filters).await?;
    let objs: Vec<serde_json::Map<String, serde_json::Value>> = rows.iter().map(|r| {
        cols.iter().cloned().zip(r.iter().cloned()).collect()
    }).collect();
    let f = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(f, &objs)?;
    Ok(rows.len() as u64)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub inserted: u64,
}

fn inserts_to_changeset(rows: Vec<serde_json::Map<String, serde_json::Value>>) -> Changeset {
    Changeset { updates: Vec::<RowUpdate>::new(), inserts: rows, deletes: vec![] }
}

pub async fn import_csv(d: &dyn SqlDriver, table: &TableInfo, path: &str) -> Result<ImportResult, AppError> {
    let mut rdr = csv::Reader::from_path(path).map_err(|e| AppError::internal(e.to_string()))?;
    let headers: Vec<String> = rdr.headers().map_err(|e| AppError::internal(e.to_string()))?
        .iter().map(|s| s.to_string()).collect();
    let mut inserts = vec![];
    for rec in rdr.records() {
        let rec = rec.map_err(|e| AppError::internal(e.to_string()))?;
        let mut map = serde_json::Map::new();
        for (h, v) in headers.iter().zip(rec.iter()) {
            // empty cell -> NULL; otherwise a string the DB coerces to the column type
            let val = if v.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(v.to_string()) };
            map.insert(h.clone(), val);
        }
        inserts.push(map);
    }
    let n = inserts.len() as u64;
    d.apply_changeset(table, &inserts_to_changeset(inserts)).await?;
    Ok(ImportResult { inserted: n })
}

pub async fn import_json(d: &dyn SqlDriver, table: &TableInfo, path: &str) -> Result<ImportResult, AppError> {
    let raw = std::fs::read_to_string(path)?;
    let arr: Vec<serde_json::Map<String, serde_json::Value>> =
        serde_json::from_str(&raw).map_err(|e| AppError::query(format!("expected a JSON array of objects: {e}")))?;
    let n = arr.len() as u64;
    d.apply_changeset(table, &inserts_to_changeset(arr)).await?;
    Ok(ImportResult { inserted: n })
}

/// Splits a SQL file into statements (string/comment/dollar-quote aware).
fn split_sql(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let n = b.len();
    let (mut i, mut start) = (0usize, 0usize);
    let mut out = vec![];
    #[derive(PartialEq)]
    enum M { Code, Sq, Dq, Line, Block, Dollar }
    let mut mode = M::Code;
    let mut tag = String::new();
    let push = |out: &mut Vec<String>, s: &str| {
        let t = s.trim();
        if !t.is_empty() { out.push(t.to_string()); }
    };
    while i < n {
        let c = b[i] as char;
        let next = if i + 1 < n { b[i + 1] as char } else { '\0' };
        match mode {
            M::Code => {
                if c == '\'' { mode = M::Sq; }
                else if c == '"' { mode = M::Dq; }
                else if c == '-' && next == '-' { mode = M::Line; i += 1; }
                else if c == '/' && next == '*' { mode = M::Block; i += 1; }
                else if c == '$' {
                    // $tag$ or $$
                    let rest = &text[i..];
                    if let Some(end) = rest[1..].find('$') {
                        let candidate = &rest[..end + 2];
                        if candidate.chars().skip(1).take(end).all(|ch| ch.is_alphanumeric() || ch == '_') {
                            tag = candidate.to_string();
                            mode = M::Dollar;
                            i += candidate.len() - 1;
                        }
                    }
                }
                else if c == ';' { push(&mut out, &text[start..i]); start = i + 1; }
            }
            M::Sq => if c == '\'' { mode = M::Code; },
            M::Dq => if c == '"' { mode = M::Code; },
            M::Line => if c == '\n' { mode = M::Code; },
            M::Block => if c == '*' && next == '/' { mode = M::Code; i += 1; },
            M::Dollar => if text[i..].starts_with(&tag) { mode = M::Code; i += tag.len() - 1; },
        }
        i += 1;
    }
    push(&mut out, &text[start..]);
    out
}

pub async fn import_sql(d: &dyn SqlDriver, path: &str) -> Result<ImportResult, AppError> {
    let raw = std::fs::read_to_string(path)?;
    let stmts = split_sql(&raw);
    let mut n = 0u64;
    for s in &stmts {
        d.execute_raw(s).await?;
        n += 1;
    }
    Ok(ImportResult { inserted: n })
}

#[cfg(test)]
mod tests {
    use super::split_sql;

    #[test]
    fn splits_respecting_strings_and_comments() {
        let sql = "INSERT INTO t VALUES ('a;b'); -- c;d\nUPDATE t SET x=1;";
        let v = split_sql(sql);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], "INSERT INTO t VALUES ('a;b')");
        assert_eq!(v[1], "-- c;d\nUPDATE t SET x=1");
    }

    #[test]
    fn dollar_quotes() {
        let v = split_sql("CREATE FUNCTION f() AS $$ BEGIN a; b; END $$ LANGUAGE plpgsql; SELECT 1;");
        assert_eq!(v.len(), 2);
    }
}
