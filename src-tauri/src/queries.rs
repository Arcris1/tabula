use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct RunningQuery {
    pub session_id: Option<String>,
    /// registry key of the connection: `window:id`
    pub connection_id: String,
    /// per-tab session key: `window:id:tab`
    pub session_key: String,
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
    /// Remove and return every running query on a given connection (`window:id`).
    /// Used to cancel in-flight queries when a connection is torn down.
    pub async fn drain_by_connection(&self, conn_key: &str) -> Vec<RunningQuery> {
        let mut map = self.inner.lock().await;
        let ids: Vec<String> = map.iter().filter(|(_, q)| q.connection_id == conn_key).map(|(k, _)| k.clone()).collect();
        ids.into_iter().filter_map(|k| map.remove(&k)).collect()
    }
    /// Remove and return every running query on a given tab session (`window:id:tab`).
    pub async fn drain_by_session_key(&self, skey: &str) -> Vec<RunningQuery> {
        let mut map = self.inner.lock().await;
        let ids: Vec<String> = map.iter().filter(|(_, q)| q.session_key == skey).map(|(k, _)| k.clone()).collect();
        ids.into_iter().filter_map(|k| map.remove(&k)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rq(conn: &str, skey: &str) -> RunningQuery {
        RunningQuery { session_id: Some("spid".into()), connection_id: conn.into(), session_key: skey.into() }
    }

    #[tokio::test]
    async fn insert_remove() {
        let reg = QueryRegistry::default();
        reg.insert("q1", rq("c", "c:t")).await;
        assert!(reg.remove("q1").await.is_some());
        assert!(reg.remove("q1").await.is_none());
    }

    #[tokio::test]
    async fn drain_by_scope() {
        let reg = QueryRegistry::default();
        reg.insert("q1", rq("win:a", "win:a:t1")).await;
        reg.insert("q2", rq("win:a", "win:a:t2")).await;
        reg.insert("q3", rq("win:b", "win:b:t1")).await;
        // by session key: only that tab's query
        let one = reg.drain_by_session_key("win:a:t1").await;
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].session_key, "win:a:t1");
        // by connection: the remaining query on win:a (q2), not win:b
        let conn = reg.drain_by_connection("win:a").await;
        assert_eq!(conn.len(), 1);
        assert_eq!(conn[0].session_key, "win:a:t2");
        assert_eq!(reg.drain_by_connection("win:b").await.len(), 1);
    }
}
