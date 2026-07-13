use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Engine { Mysql, Postgres, Sqlite, Redis, Mssql }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvColor { Gray, Blue, Green, Amber, Red }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Some => key auth (ssh secret = passphrase); None => password auth
    pub key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub color: EnvColor,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    /// database name for MySQL/Postgres, file path for SQLite, db index (as string) for Redis
    pub database: Option<String>,
    pub is_production: bool,
    #[serde(default)]
    pub ssh: Option<SshConfig>,
}

pub struct ConfigStore { path: PathBuf }

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self { Self { path } }

    pub fn list(&self) -> Result<Vec<ConnectionConfig>, AppError> {
        if !self.path.exists() { return Ok(vec![]); }
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn upsert(&self, cfg: ConnectionConfig) -> Result<(), AppError> {
        let mut all = self.list()?;
        all.retain(|c| c.id != cfg.id);
        all.push(cfg);
        self.write(&all)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut all = self.list()?;
        all.retain(|c| c.id != id);
        self.write(&all)
    }

    fn write(&self, all: &[ConnectionConfig]) -> Result<(), AppError> {
        if let Some(dir) = self.path.parent() { std::fs::create_dir_all(dir)?; }
        std::fs::write(&self.path, serde_json::to_string_pretty(all)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(id: &str, name: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(), name: name.into(), engine: Engine::Sqlite,
            color: EnvColor::Gray, host: None, port: None, username: None,
            database: Some("/tmp/x.db".into()), is_production: false, ssh: None,
        }
    }

    #[test]
    fn old_configs_without_ssh_still_load() {
        let raw = r#"[{"id":"a","name":"x","engine":"sqlite","color":"gray","host":null,"port":null,"username":null,"database":"/tmp/x.db","isProduction":false}]"#;
        let parsed: Vec<ConnectionConfig> = serde_json::from_str(raw).unwrap();
        assert!(parsed[0].ssh.is_none());
    }

    #[test]
    fn upsert_list_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("connections.json"));
        assert!(store.list().unwrap().is_empty());
        store.upsert(cfg("a", "one")).unwrap();
        store.upsert(cfg("b", "two")).unwrap();
        store.upsert(cfg("a", "one-renamed")).unwrap(); // upsert replaces
        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().find(|c| c.id == "a").unwrap().name, "one-renamed");
        store.delete("a").unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn engine_serializes_lowercase() {
        assert_eq!(serde_json::to_value(Engine::Postgres).unwrap(), "postgres");
        assert_eq!(serde_json::to_value(EnvColor::Red).unwrap(), "red");
    }
}
