use crate::error::AppError;
use std::collections::HashMap;
use std::path::PathBuf;

/// Persisted SSH host-key fingerprints, keyed "host:port" (known_hosts.json).
/// Trust-on-first-use: the first fingerprint seen is recorded and every later
/// connection must match it, so a swapped server key (MITM) is rejected.
pub struct KnownHostsStore {
    path: PathBuf,
}

impl KnownHostsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<HashMap<String, String>, AppError> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn write(&self, map: &HashMap<String, String>) -> Result<(), AppError> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(map)?)?;
        Ok(())
    }

    pub fn get(&self, host: &str, port: u16) -> Result<Option<String>, AppError> {
        Ok(self.read()?.remove(&format!("{host}:{port}")))
    }

    pub fn put(&self, host: &str, port: u16, fingerprint: &str) -> Result<(), AppError> {
        let mut map = self.read()?;
        map.insert(format!("{host}:{port}"), fingerprint.to_string());
        self.write(&map)
    }

    /// Forgets a recorded key so the next connection re-trusts on first use
    /// (use after a legitimate server key rotation).
    pub fn forget(&self, host: &str, port: u16) -> Result<(), AppError> {
        let mut map = self.read()?;
        map.remove(&format!("{host}:{port}"));
        self.write(&map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tofu_record_match_forget() {
        let dir = tempfile::tempdir().unwrap();
        let s = KnownHostsStore::new(dir.path().join("known_hosts.json"));
        assert_eq!(s.get("h", 22).unwrap(), None);
        s.put("h", 22, "SHA256:aaa").unwrap();
        assert_eq!(s.get("h", 22).unwrap(), Some("SHA256:aaa".into()));
        // a different host is independent
        assert_eq!(s.get("h", 2222).unwrap(), None);
        s.forget("h", 22).unwrap();
        assert_eq!(s.get("h", 22).unwrap(), None);
    }
}
