//! `.miwa/lock.json`: agreed-upon hashes for each edge.

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LockEntry {
    pub from: String,
    pub to: String,
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub at: String,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Lock {
    pub edges: std::collections::BTreeMap<String, LockEntry>,
}

impl Lock {
    pub fn parse(json: &str) -> anyhow::Result<Lock> {
        let _ = json;
        todo!()
    }

    /// Pretty JSON, stable key order, trailing newline.
    pub fn to_json(&self) -> String {
        todo!()
    }

    /// Returns `Lock::default()` if the file does not exist.
    pub fn load(root: &std::path::Path) -> anyhow::Result<Lock> {
        let _ = root;
        todo!()
    }

    /// Loads via a `Source` (for `GitTreeSource`).
    pub fn load_from_source(source: &dyn crate::source::Source) -> anyhow::Result<Lock> {
        let _ = source;
        todo!()
    }

    /// Writes via a temp file + rename.
    pub fn save(&self, root: &std::path::Path) -> anyhow::Result<()> {
        let _ = root;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // [FR-021]
    #[test]
    fn ロックファイルが存在しない場合は空のロックとして扱う() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Lock::load(dir.path()).unwrap();
        assert!(lock.edges.is_empty());
    }

    // [FR-021]
    #[test]
    fn 壊れたjsonのロックはparseがエラーになる() {
        assert!(Lock::parse("{ not valid json").is_err());
        assert!(Lock::parse(r#"{"edges": "not-an-object"}"#).is_err());
    }

    // [FR-022]
    #[test]
    fn to_jsonはエッジidの昇順で出力される() {
        let mut lock = Lock::default();
        let make_entry = |from: &str, to: &str| LockEntry {
            from: from.to_string(),
            to: to.to_string(),
            by: "human".to_string(),
            reason: None,
            score: None,
            at: "2026-01-01T00:00:00Z".to_string(),
        };
        // 挿入順はキー順とは無関係にしておく
        lock.edges.insert(
            "doc-req:DOC-Z:FR-002".to_string(),
            make_entry("sha256:z", "sha256:2"),
        );
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            make_entry("sha256:a", "sha256:1"),
        );
        lock.edges.insert(
            "test-req:src/a:FR-001".to_string(),
            make_entry("sha256:t", "sha256:1"),
        );

        let json = lock.to_json();
        let pos_a = json.find("doc-req:DOC-A:FR-001").unwrap();
        let pos_z = json.find("doc-req:DOC-Z:FR-002").unwrap();
        let pos_t = json.find("test-req:src/a:FR-001").unwrap();
        assert!(
            pos_a < pos_z,
            "doc-req:DOC-A should come before doc-req:DOC-Z: {json}"
        );
        assert!(
            pos_z < pos_t,
            "doc-req:DOC-Z should come before test-req:src/a: {json}"
        );

        // 往復させてもキー順は保たれる
        let reparsed = Lock::parse(&json).unwrap();
        let keys: Vec<&String> = reparsed.edges.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
