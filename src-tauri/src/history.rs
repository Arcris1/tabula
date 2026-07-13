use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CAP_PER_CONNECTION: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub connection_id: String,
    pub sql: String,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub status: String, // "ok" | "error"
    pub row_count: Option<i64>,
    pub error: Option<String>,
}

pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_all(&self) -> Result<Vec<HistoryEntry>, AppError> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn list(&self, connection_id: &str) -> Result<Vec<HistoryEntry>, AppError> {
        let mut entries: Vec<HistoryEntry> = self
            .read_all()?
            .into_iter()
            .filter(|e| e.connection_id == connection_id)
            .collect();
        entries.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms)); // newest first
        Ok(entries)
    }

    pub fn add(&self, entry: HistoryEntry) -> Result<(), AppError> {
        let mut all = self.read_all()?;
        all.push(entry);
        // cap per connection: keep the newest CAP entries for each connection
        let mut by_conn: std::collections::HashMap<String, Vec<HistoryEntry>> = Default::default();
        for e in all {
            by_conn.entry(e.connection_id.clone()).or_default().push(e);
        }
        let mut kept: Vec<HistoryEntry> = vec![];
        for (_, mut entries) in by_conn {
            entries.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
            entries.truncate(CAP_PER_CONNECTION);
            kept.extend(entries);
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string(&kept)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(conn: &str, sql: &str, t: i64) -> HistoryEntry {
        HistoryEntry {
            connection_id: conn.into(), sql: sql.into(), started_at_ms: t,
            duration_ms: 5, status: "ok".into(), row_count: Some(1), error: None,
        }
    }

    #[test]
    fn add_list_newest_first_and_per_connection() {
        let dir = tempfile::tempdir().unwrap();
        let s = HistoryStore::new(dir.path().join("history.json"));
        s.add(e("a", "select 1", 100)).unwrap();
        s.add(e("a", "select 2", 200)).unwrap();
        s.add(e("b", "select 3", 300)).unwrap();
        let a = s.list("a").unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].sql, "select 2"); // newest first
        assert_eq!(s.list("b").unwrap().len(), 1);
    }

    #[test]
    fn caps_at_200_per_connection() {
        let dir = tempfile::tempdir().unwrap();
        let s = HistoryStore::new(dir.path().join("history.json"));
        for i in 0..205 {
            s.add(e("a", &format!("q{i}"), i)).unwrap();
        }
        let a = s.list("a").unwrap();
        assert_eq!(a.len(), 200);
        assert_eq!(a[0].sql, "q204");
    }
}
