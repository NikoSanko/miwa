//! Graph domain model: nodes, edges, and validity/brokenness findings.

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum NodeKind {
    Doc,
    Req,
    Test,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct NodeKey {
    pub kind: NodeKind,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub key: NodeKey,
    pub path: String,
    pub content: String,
    pub hash: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeKind {
    DocReq,
    TestReq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub id: String,
    pub kind: EdgeKind,
    pub from: NodeKey,
    pub to: NodeKey,
}

/// `"doc-req:<from>:<to>"` / `"test-req:<from>:<to>"`.
pub fn edge_id(kind: EdgeKind, from_id: &str, to_id: &str) -> String {
    let _ = (kind, from_id, to_id);
    todo!()
}

/// `id` is non-empty and contains only `[A-Za-z0-9._-]`.
pub fn is_valid_id(id: &str) -> bool {
    let _ = id;
    todo!()
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum TargetKind {
    Doc,
    Req,
    Test,
    Edge,
    File,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ReasonCode {
    DocChanged,
    ReqChanged,
    TestChanged,
    BothChanged,
    NewEdge,
    RemovedEdge,
    MissingReq,
    NotReqFile,
    ReqWithoutDoc,
    DocWithoutReq,
    ReqWithoutTest,
    DuplicateId,
    MissingId,
    InvalidId,
    MultipleMarkers,
    ParseError,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Invalid {
    pub target_kind: TargetKind,
    pub target: String,
    pub reason: ReasonCode,
    pub detail: String,
}

/// `target` is a req id or a link-target path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Broken {
    pub from: NodeKey,
    pub target: String,
    pub reason: ReasonCode,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: std::collections::BTreeMap<NodeKey, Node>,
    pub edges: std::collections::BTreeMap<String, Edge>,
    pub invalid: Vec<Invalid>,
    pub broken: Vec<Broken>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // [FR-023]
    #[test]
    fn edge_idはdoc_reqの接頭辞と両端のidを持つ() {
        assert_eq!(
            edge_id(EdgeKind::DocReq, "DOC-AUTH", "FR-001"),
            "doc-req:DOC-AUTH:FR-001"
        );
    }

    // [FR-023]
    #[test]
    fn edge_idはtest_reqの接頭辞と両端のidを持つ() {
        assert_eq!(
            edge_id(EdgeKind::TestReq, "internal/auth:TestLogin", "FR-001"),
            "test-req:internal/auth:TestLogin:FR-001"
        );
    }

    // [FR-006]
    #[test]
    fn is_valid_idは英数字とドット_アンダースコア_ハイフンのみ許可する() {
        assert!(is_valid_id("FR-001"));
        assert!(is_valid_id("DOC_AUTH.v2"));
        assert!(is_valid_id("a"));
    }

    // [FR-006]
    #[test]
    fn is_valid_idはコロンや空白_日本語_空文字を拒否する() {
        assert!(!is_valid_id("FR:001"));
        assert!(!is_valid_id("FR 001"));
        assert!(!is_valid_id("要件001"));
        assert!(!is_valid_id(""));
    }
}
