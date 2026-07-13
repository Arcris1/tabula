use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyTreeNode {
    pub name: String,
    pub full_key: Option<String>,
    pub children: Vec<KeyTreeNode>,
    pub count: usize,
}

#[derive(Default)]
struct Builder {
    is_key: Option<String>,
    children: BTreeMap<String, Builder>,
    count: usize,
}

pub fn build(keys: &[String]) -> Vec<KeyTreeNode> {
    let mut root = Builder::default();
    for key in keys {
        let mut node = &mut root;
        node.count += 1;
        for part in key.split(':') {
            node = node.children.entry(part.to_string()).or_default();
            node.count += 1;
        }
        node.is_key = Some(key.clone());
    }
    finish(root.children)
}

fn finish(children: BTreeMap<String, Builder>) -> Vec<KeyTreeNode> {
    children.into_iter().map(|(name, b)| KeyTreeNode {
        name,
        full_key: b.is_key,
        children: finish(b.children),
        count: b.count,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_namespace_tree() {
        let keys = vec![
            "cache:users:1".to_string(),
            "cache:users:2".to_string(),
            "cache:posts:9".to_string(),
            "session_abc".to_string(),
        ];
        let tree = build(&keys);
        assert_eq!(tree.len(), 2); // "cache", "session_abc"
        let cache = tree.iter().find(|n| n.name == "cache").unwrap();
        assert_eq!(cache.count, 3);
        assert_eq!(cache.full_key, None);
        let users = cache.children.iter().find(|n| n.name == "users").unwrap();
        assert_eq!(users.count, 2);
        assert_eq!(users.children[0].full_key.as_deref(), Some("cache:users:1"));
        let session = tree.iter().find(|n| n.name == "session_abc").unwrap();
        assert_eq!(session.full_key.as_deref(), Some("session_abc"));
    }

    #[test]
    fn node_can_be_both_key_and_namespace() {
        let keys = vec!["a".to_string(), "a:b".to_string()];
        let tree = build(&keys);
        let a = &tree[0];
        assert_eq!(a.full_key.as_deref(), Some("a"));
        assert_eq!(a.children.len(), 1);
        assert_eq!(a.count, 2);
    }
}
