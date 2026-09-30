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
        let lock: Lock = serde_json::from_str(json)?;
        Ok(lock)
    }

    /// Pretty JSON, stable key order, trailing newline.
    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("Lock serializes to json");
        json.push('\n');
        json
    }

    /// Returns `Lock::default()` if the file does not exist.
    pub fn load(root: &std::path::Path) -> anyhow::Result<Lock> {
        let path = root.join(crate::config::LOCK_PATH);
        if !path.exists() {
            return Ok(Lock::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Lock::parse(&content)
    }

    /// Loads via a `Source` (for `GitTreeSource`). `.miwa/` is excluded from
    /// `list_files`, so this reads the lock file directly. A `NotFound` read
    /// error means "no lock yet"; any other error (e.g. a `git` failure)
    /// propagates instead of being silently treated as "absent".
    pub fn load_from_source(source: &dyn crate::source::Source) -> anyhow::Result<Lock> {
        let bytes = match source.read(crate::config::LOCK_PATH) {
            Ok(bytes) => bytes,
            Err(e) if crate::source::is_not_found(&e) => return Ok(Lock::default()),
            Err(e) => return Err(e),
        };
        let content = String::from_utf8(bytes)?;
        Lock::parse(&content)
    }

    /// Writes via a temp file + rename.
    pub fn save(&self, root: &std::path::Path) -> anyhow::Result<()> {
        let dir = root.join(".miwa");
        std::fs::create_dir_all(&dir)?;
        let final_path = root.join(crate::config::LOCK_PATH);
        let tmp_path = dir.join("lock.json.tmp");
        std::fs::write(&tmp_path, self.to_json())?;
        std::fs::rename(&tmp_path, &final_path)?;
        Ok(())
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

    /// A `Source` test double whose `read` always fails with a fixed error.
    struct FailingSource {
        error_kind: std::io::ErrorKind,
    }

    impl crate::source::Source for FailingSource {
        fn list_files(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
            Err(anyhow::Error::new(std::io::Error::new(
                self.error_kind,
                format!("{path} is unavailable"),
            )))
        }
    }

    // [FR-021]
    #[test]
    fn load_from_sourceはnotfoundなら空ロックとして扱う() {
        let source = FailingSource {
            error_kind: std::io::ErrorKind::NotFound,
        };
        let lock = Lock::load_from_source(&source).unwrap();
        assert!(lock.edges.is_empty());
    }

    // [FR-021]
    #[test]
    fn load_from_sourceはnotfound以外のエラーを伝播する() {
        let source = FailingSource {
            error_kind: std::io::ErrorKind::PermissionDenied,
        };
        let result = Lock::load_from_source(&source);
        assert!(
            result.is_err(),
            "non-NotFound read errors must propagate, not be treated as an absent lock"
        );
    }
}
