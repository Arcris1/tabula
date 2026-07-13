use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct RunningQuery {
    pub session_id: Option<String>,
    pub connection_id: String,
}

#[derive(Default)]
pub struct QueryRegistry {
    inner: Mutex<HashMap<String, RunningQuery>>,
}

impl QueryRegistry {
    pub async fn insert(&self, id: &str, q: RunningQuery) {
        self.inner.lock().await.insert(id.to_string(), q);
    }
    pub async fn remove(&self, id: &str) -> Option<RunningQuery> {
        self.inner.lock().await.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_remove() {
        let reg = QueryRegistry::default();
        reg.insert("q1", RunningQuery { session_id: None, connection_id: "c".into() }).await;
        assert!(reg.remove("q1").await.is_some());
        assert!(reg.remove("q1").await.is_none());
    }
}
