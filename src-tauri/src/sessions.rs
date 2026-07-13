use crate::drivers::QuerySession;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Persistent per-query-tab connections, keyed "{window}:{connection}:{tab}".
/// A session is TAKEN while a statement streams and PUT back afterwards, so a
/// tab's statements all share one connection (BEGIN/COMMIT/ROLLBACK work).
#[derive(Default)]
pub struct SessionRegistry {
    inner: Mutex<HashMap<String, Box<dyn QuerySession>>>,
}

impl SessionRegistry {
    pub async fn take(&self, key: &str) -> Option<Box<dyn QuerySession>> {
        self.inner.lock().await.remove(key)
    }
    pub async fn put(&self, key: &str, session: Box<dyn QuerySession>) {
        self.inner.lock().await.insert(key.to_string(), session);
    }
    pub async fn remove(&self, key: &str) -> Option<Box<dyn QuerySession>> {
        self.inner.lock().await.remove(key)
    }
    /// removes and returns every session whose key starts with `prefix`
    pub async fn drain_by_prefix(&self, prefix: &str) -> Vec<Box<dyn QuerySession>> {
        let mut map = self.inner.lock().await;
        let keys: Vec<String> = map.keys().filter(|k| k.starts_with(prefix)).cloned().collect();
        keys.into_iter().filter_map(|k| map.remove(&k)).collect()
    }
    /// removes and returns every session whose key contains `infix`
    pub async fn drain_by_infix(&self, infix: &str) -> Vec<Box<dyn QuerySession>> {
        let mut map = self.inner.lock().await;
        let keys: Vec<String> = map.keys().filter(|k| k.contains(infix)).cloned().collect();
        keys.into_iter().filter_map(|k| map.remove(&k)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::sqlite::SqliteDriver;
    use crate::drivers::SqlDriver;

    #[tokio::test]
    async fn take_put_and_sweeps() {
        let reg = SessionRegistry::default();
        let d = SqliteDriver::connect(":memory:").await.unwrap();
        reg.put("winA:conn1:tab1", d.acquire_session().await.unwrap()).await;
        reg.put("winA:conn2:tab2", d.acquire_session().await.unwrap()).await;
        reg.put("winB:conn1:tab3", d.acquire_session().await.unwrap()).await;

        let taken = reg.take("winA:conn1:tab1").await;
        assert!(taken.is_some());
        assert!(reg.take("winA:conn1:tab1").await.is_none()); // exclusive while streaming
        reg.put("winA:conn1:tab1", taken.unwrap()).await;

        // window close sweeps only that window's sessions
        let drained = reg.drain_by_prefix("winA:").await;
        assert_eq!(drained.len(), 2);
        // connection delete sweeps across windows
        let drained = reg.drain_by_infix(":conn1:").await;
        assert_eq!(drained.len(), 1);
    }
}
