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
    /// Normalized front matter `description` (Doc / Req only).
    pub description: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeKind {
    DocReq,
    TestReq,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub id: String,
    pub kind: EdgeKind,
    pub from: NodeKey,
    pub to: NodeKey,
}

/// `"doc-req:<from>:<to>"` / `"test-req:<from>:<to>"`. `Desc` edges have a
/// different shape; use [`desc_edge_id`] for them.
pub fn edge_id(kind: EdgeKind, from_id: &str, to_id: &str) -> String {
    let prefix = match kind {
        EdgeKind::DocReq => "doc-req",
        EdgeKind::TestReq => "test-req",
        EdgeKind::Desc => unreachable!("use desc_edge_id"),
    };
    format!("{prefix}:{from_id}:{to_id}")
}

/// `"desc:doc:<id>"` for a Doc node, `"desc:req:<id>"` for a Req node. The
/// node kind is part of the id because a Doc and a Req may share an `id`.
/// Test nodes never have a `desc` edge.
pub fn desc_edge_id(key: &NodeKey) -> String {
    let prefix = match key.kind {
        NodeKind::Doc => "desc:doc",
        NodeKind::Req => "desc:req",
        NodeKind::Test => unreachable!("test nodes have no desc edge"),
    };
    format!("{prefix}:{}", key.id)
}

impl Node {
    /// The normalized `description`, or `""` when there is none.
    pub fn description_text(&self) -> &str {
        self.description.as_deref().unwrap_or("")
    }
}

/// The `(from, to)` hashes recorded in the lock for `edge`. DocReq / TestReq
/// use the hashes of both endpoint nodes (empty if the node is missing);
/// Desc uses `(hash of the description, hash of the body)` of the from node.
/// Every edge in `graph.edges` has its nodes, so the missing-node results
/// are unreachable in practice.
pub fn edge_hashes(graph: &Graph, edge: &Edge) -> (String, String) {
    match edge.kind {
        EdgeKind::DocReq | EdgeKind::TestReq => {
            let h = |k: &NodeKey| {
                graph
                    .nodes
                    .get(k)
                    .map(|n| n.hash.clone())
                    .unwrap_or_default()
            };
            (h(&edge.from), h(&edge.to))
        }
        EdgeKind::Desc => match graph.nodes.get(&edge.from) {
            Some(node) => (
                crate::hash::content_hash(node.description_text()),
                node.hash.clone(),
            ),
            None => (String::new(), String::new()),
        },
    }
}

/// `id` is non-empty and contains only `[A-Za-z0-9._-]`.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
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
    Index,
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
    MissingIndex,
    NotListed,
    UntypedFile,
    BrokenEntry,
    TitleMismatch,
    DescriptionMismatch,
    MissingDescription,
    MissingDirectoryDescription,
    UnlinkedIndex,
    DescriptionChanged,
    BodyChanged,
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
    pub index_issues: Vec<crate::index::IndexIssue>,
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

    // [FR-023]
    #[test]
    fn desc_edge_idはノード種別の接頭辞とノードのidを持つ() {
        let key = |kind, id: &str| NodeKey {
            kind,
            id: id.to_string(),
        };
        assert_eq!(desc_edge_id(&key(NodeKind::Doc, "DOC-A")), "desc:doc:DOC-A");
        assert_eq!(
            desc_edge_id(&key(NodeKind::Req, "FR-001")),
            "desc:req:FR-001"
        );
        assert_ne!(
            desc_edge_id(&key(NodeKind::Doc, "X-1")),
            desc_edge_id(&key(NodeKind::Req, "X-1"))
        );
    }

    fn desc_node(description: Option<&str>, hash: &str) -> Node {
        Node {
            key: NodeKey {
                kind: NodeKind::Doc,
                id: "DOC-A".to_string(),
            },
            path: "docs/a.md".to_string(),
            content: "本文".to_string(),
            hash: hash.to_string(),
            description: description.map(str::to_string),
        }
    }

    fn desc_graph(description: Option<&str>) -> (Graph, Edge) {
        let node = desc_node(description, "sha256:body");
        let edge = Edge {
            id: "desc:doc:DOC-A".to_string(),
            kind: EdgeKind::Desc,
            from: node.key.clone(),
            to: node.key.clone(),
        };
        let mut graph = Graph::default();
        graph.nodes.insert(node.key.clone(), node);
        graph.edges.insert(edge.id.clone(), edge.clone());
        (graph, edge)
    }

    // [FR-087]
    #[test]
    fn edge_hashesのdescは要約のハッシュと本文のハッシュを返す() {
        let (graph, edge) = desc_graph(Some("要約です"));
        assert_eq!(
            edge_hashes(&graph, &edge),
            (
                crate::hash::content_hash("要約です"),
                "sha256:body".to_string()
            )
        );
    }

    // [FR-087]
    #[test]
    fn edge_hashesのdescは要約が無ければ空文字列のハッシュをfromにする() {
        let (graph, edge) = desc_graph(None);
        assert_eq!(
            edge_hashes(&graph, &edge),
            (crate::hash::content_hash(""), "sha256:body".to_string())
        );
    }

    // [FR-087]
    #[test]
    fn edge_hashesのdescは要約を変えるとfromだけが変わる() {
        let (g1, e1) = desc_graph(Some("旧"));
        let (g2, e2) = desc_graph(Some("新"));
        let (f1, t1) = edge_hashes(&g1, &e1);
        let (f2, t2) = edge_hashes(&g2, &e2);
        assert_ne!(f1, f2);
        assert_eq!(t1, t2);
    }

    // [FR-087]
    #[test]
    fn edge_hashesのdoc_reqは要約を変えても変わらない() {
        let build = |description: Option<&str>| {
            let mut graph = Graph::default();
            let doc = desc_node(description, "sha256:doc");
            let req = Node {
                key: NodeKey {
                    kind: NodeKind::Req,
                    id: "FR-001".to_string(),
                },
                path: "req/fr1.md".to_string(),
                content: String::new(),
                hash: "sha256:req".to_string(),
                description: None,
            };
            let edge = Edge {
                id: "doc-req:DOC-A:FR-001".to_string(),
                kind: EdgeKind::DocReq,
                from: doc.key.clone(),
                to: req.key.clone(),
            };
            graph.nodes.insert(doc.key.clone(), doc);
            graph.nodes.insert(req.key.clone(), req);
            (graph, edge)
        };
        let (g1, e1) = build(Some("旧"));
        let (g2, e2) = build(Some("新"));
        assert_eq!(edge_hashes(&g1, &e1), edge_hashes(&g2, &e2));
        assert_eq!(
            edge_hashes(&g1, &e1),
            ("sha256:doc".to_string(), "sha256:req".to_string())
        );
    }
}
