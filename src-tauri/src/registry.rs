use crate::drivers::{KvDriver, SqlDriver};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub enum LiveConnection {
    Sql(Box<dyn SqlDriver>),
    Kv(Box<dyn KvDriver>),
}

#[derive(Default)]
pub struct ConnectionRegistry {
    inner: Mutex<HashMap<String, Arc<LiveConnection>>>,
}

impl ConnectionRegistry {
    pub async fn insert(&self, id: &str, conn: LiveConnection) {
        self.inner.lock().await.insert(id.to_string(), Arc::new(conn));
    }
    pub async fn get(&self, id: &str) -> Option<Arc<LiveConnection>> {
        self.inner.lock().await.get(id).cloned()
    }
    pub async fn remove(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }
    /// removes every window's entry for a connection (keys end with ":{id}")
    pub async fn remove_by_connection(&self, id: &str) {
        let suffix = format!(":{id}");
        self.inner.lock().await.retain(|k, _| !k.ends_with(&suffix));
    }
    /// removes every connection owned by a window (keys start with "{label}:")
    pub async fn remove_by_window(&self, label: &str) {
        let prefix = format!("{label}:");
        self.inner.lock().await.retain(|k, _| !k.starts_with(&prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::sqlite::SqliteDriver;

    #[tokio::test]
    async fn window_scoped_keys_are_independent() {
        let reg = ConnectionRegistry::default();
        let a = SqliteDriver::connect(":memory:").await.unwrap();
        let b = SqliteDriver::connect(":memory:").await.unwrap();
        reg.insert("winA:conn1", LiveConnection::Sql(Box::new(a))).await;
        reg.insert("winB:conn1", LiveConnection::Sql(Box::new(b))).await;

        // one window closing must not touch the other
        reg.remove_by_window("winA").await;
        assert!(reg.get("winA:conn1").await.is_none());
        assert!(reg.get("winB:conn1").await.is_some());

        // deleting the saved connection sweeps every window
        reg.remove_by_connection("conn1").await;
        assert!(reg.get("winB:conn1").await.is_none());
    }

    #[tokio::test]
    async fn insert_get_remove() {
        let reg = ConnectionRegistry::default();
        let d = SqliteDriver::connect(":memory:").await.unwrap();
        reg.insert("c1", LiveConnection::Sql(Box::new(d))).await;
        assert!(reg.get("c1").await.is_some());
        assert!(matches!(&*reg.get("c1").await.unwrap(), LiveConnection::Sql(_)));
        reg.remove("c1").await;
        assert!(reg.get("c1").await.is_none());
    }
}
