//! `check`: compare the current graph against the lock and report findings.

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FindingKind {
    Drift,
    New,
    Removed,
    Broken,
    Orphan,
    NoTest,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub target_kind: crate::model::TargetKind,
    pub target: String,
    pub reason: crate::model::ReasonCode,
    pub detail: String,
}

/// Findings are sorted by `(kind, target_kind, target, reason)`.
pub fn check(graph: &crate::model::Graph, lock: &crate::lock::Lock) -> Vec<Finding> {
    let _ = (graph, lock);
    todo!()
}

/// `current - base`, keyed by `(kind, target_kind, target, reason)`.
pub fn diff_findings(current: &[Finding], base: &[Finding]) -> Vec<Finding> {
    let _ = (current, base);
    todo!()
}

/// Returns 2 if `gate` and `findings` is non-empty, else 0.
pub fn exit_code(findings: &[Finding], gate: bool) -> i32 {
    let _ = (findings, gate);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::{Lock, LockEntry};
    use crate::model::{Edge, EdgeKind, Graph, Node, NodeKey, NodeKind, ReasonCode};

    fn node(kind: NodeKind, id: &str, hash: &str) -> Node {
        Node {
            key: NodeKey {
                kind,
                id: id.to_string(),
            },
            path: format!("{id}.md"),
            content: String::new(),
            hash: hash.to_string(),
        }
    }

    fn lock_entry(from: &str, to: &str) -> LockEntry {
        LockEntry {
            from: from.to_string(),
            to: to.to_string(),
            by: "human".to_string(),
            reason: None,
            score: None,
            at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn doc_req_graph(doc_hash: &str, req_hash: &str) -> Graph {
        let mut graph = Graph::default();
        let doc_key = NodeKey {
            kind: NodeKind::Doc,
            id: "DOC-A".to_string(),
        };
        let req_key = NodeKey {
            kind: NodeKind::Req,
            id: "FR-001".to_string(),
        };
        graph
            .nodes
            .insert(doc_key.clone(), node(NodeKind::Doc, "DOC-A", doc_hash));
        graph
            .nodes
            .insert(req_key.clone(), node(NodeKind::Req, "FR-001", req_hash));
        let edge_id = "doc-req:DOC-A:FR-001".to_string();
        graph.edges.insert(
            edge_id.clone(),
            Edge {
                id: edge_id,
                kind: EdgeKind::DocReq,
                from: doc_key,
                to: req_key,
            },
        );
        graph
    }

    fn test_req_graph(test_hash: &str, req_hash: &str) -> Graph {
        let mut graph = Graph::default();
        let test_key = NodeKey {
            kind: NodeKind::Test,
            id: "src/foo::bar".to_string(),
        };
        let req_key = NodeKey {
            kind: NodeKind::Req,
            id: "FR-001".to_string(),
        };
        graph.nodes.insert(
            test_key.clone(),
            node(NodeKind::Test, "src/foo::bar", test_hash),
        );
        graph
            .nodes
            .insert(req_key.clone(), node(NodeKind::Req, "FR-001", req_hash));
        let edge_id = "test-req:src/foo::bar:FR-001".to_string();
        graph.edges.insert(
            edge_id.clone(),
            Edge {
                id: edge_id,
                kind: EdgeKind::TestReq,
                from: test_key,
                to: req_key,
            },
        );
        graph
    }

    // [FR-024]
    #[test]
    fn testだけ変わった場合はtest_changedとしてdriftを報告する() {
        let graph = test_req_graph("sha256:test-new", "sha256:req-same");
        let mut lock = Lock::default();
        lock.edges.insert(
            "test-req:src/foo::bar:FR-001".to_string(),
            lock_entry("sha256:test-old", "sha256:req-same"),
        );
        let findings = check(&graph, &lock);
        assert!(findings.iter().any(|f| {
            f.kind == FindingKind::Drift
                && f.reason == ReasonCode::TestChanged
                && f.target == "test-req:src/foo::bar:FR-001"
        }));
    }

    // [FR-024]
    #[test]
    fn docだけ変わった場合はdoc_changedとしてdriftを報告する() {
        let graph = doc_req_graph("sha256:doc-new", "sha256:req-same");
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry("sha256:doc-old", "sha256:req-same"),
        );
        let findings = check(&graph, &lock);
        assert!(findings.iter().any(|f| f.kind == FindingKind::Drift
            && f.reason == ReasonCode::DocChanged
            && f.target == "doc-req:DOC-A:FR-001"));
    }

    // [FR-024]
    #[test]
    fn reqだけ変わった場合はreq_changedとしてdriftを報告する() {
        let graph = doc_req_graph("sha256:doc-same", "sha256:req-new");
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry("sha256:doc-same", "sha256:req-old"),
        );
        let findings = check(&graph, &lock);
        assert!(findings
            .iter()
            .any(|f| f.kind == FindingKind::Drift && f.reason == ReasonCode::ReqChanged));
    }

    // [FR-024]
    #[test]
    fn 両側が変わった場合はboth_changedとして1件だけ報告する() {
        let graph = doc_req_graph("sha256:doc-new", "sha256:req-new");
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry("sha256:doc-old", "sha256:req-old"),
        );
        let findings = check(&graph, &lock);
        let drifts: Vec<_> = findings
            .iter()
            .filter(|f| f.kind == FindingKind::Drift && f.target == "doc-req:DOC-A:FR-001")
            .collect();
        assert_eq!(
            drifts.len(),
            1,
            "expected exactly one drift finding: {drifts:?}"
        );
        assert_eq!(drifts[0].reason, ReasonCode::BothChanged);
    }

    // [FR-025]
    #[test]
    fn ロックにないエッジはnewとして報告する() {
        let graph = doc_req_graph("sha256:doc", "sha256:req");
        let lock = Lock::default();
        let findings = check(&graph, &lock);
        assert!(findings.iter().any(|f| f.kind == FindingKind::New
            && f.target == "doc-req:DOC-A:FR-001"
            && f.reason == ReasonCode::NewEdge));
    }

    // [FR-025]
    #[test]
    fn ソースから消えたエッジはremovedとして報告する() {
        let mut graph = Graph::default();
        graph.nodes.insert(
            NodeKey {
                kind: NodeKind::Req,
                id: "FR-001".to_string(),
            },
            node(NodeKind::Req, "FR-001", "sha256:req"),
        );
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry("sha256:doc-old", "sha256:req-old"),
        );
        let findings = check(&graph, &lock);
        assert!(findings.iter().any(|f| {
            f.kind == FindingKind::Removed
                && f.target == "doc-req:DOC-A:FR-001"
                && f.reason == ReasonCode::RemovedEdge
        }));
    }

    // [FR-026]
    #[test]
    fn どのdocからもリンクされないreqはorphanになる() {
        let mut graph = Graph::default();
        graph.nodes.insert(
            NodeKey {
                kind: NodeKind::Req,
                id: "FR-001".to_string(),
            },
            node(NodeKind::Req, "FR-001", "sha256:req"),
        );
        let findings = check(&graph, &Lock::default());
        assert!(findings.iter().any(|f| {
            f.kind == FindingKind::Orphan
                && f.reason == ReasonCode::ReqWithoutDoc
                && f.target == "FR-001"
        }));
    }

    // [FR-026]
    #[test]
    fn どのreqにもリンクしないdocはorphanになる() {
        let mut graph = Graph::default();
        graph.nodes.insert(
            NodeKey {
                kind: NodeKind::Doc,
                id: "DOC-A".to_string(),
            },
            node(NodeKind::Doc, "DOC-A", "sha256:doc"),
        );
        let findings = check(&graph, &Lock::default());
        assert!(findings.iter().any(|f| {
            f.kind == FindingKind::Orphan
                && f.reason == ReasonCode::DocWithoutReq
                && f.target == "DOC-A"
        }));
    }

    // [FR-027]
    #[test]
    fn テストで検証されていないreqはno_testになる() {
        let graph = doc_req_graph("sha256:doc", "sha256:req");
        let findings = check(&graph, &Lock::default());
        assert!(findings.iter().any(|f| {
            f.kind == FindingKind::NoTest
                && f.reason == ReasonCode::ReqWithoutTest
                && f.target == "FR-001"
        }));
    }

    // [FR-030]
    #[test]
    fn exit_codeはgate指定かつfindingがあれば2を返す() {
        let graph = doc_req_graph("sha256:doc", "sha256:req");
        let findings = check(&graph, &Lock::default());
        assert!(!findings.is_empty());
        assert_eq!(exit_code(&findings, true), 2);
    }

    // [FR-030]
    #[test]
    fn exit_codeはgateなしなら常に0を返す() {
        let graph = doc_req_graph("sha256:doc", "sha256:req");
        let findings = check(&graph, &Lock::default());
        assert!(!findings.is_empty());
        assert_eq!(exit_code(&findings, false), 0);
        assert_eq!(exit_code(&[], false), 0);
    }

    // [FR-030]
    #[test]
    fn exit_codeはfindingが0件でgateがあっても0を返す() {
        assert_eq!(exit_code(&[], true), 0);
    }

    // [FR-031]
    #[test]
    fn diff_findingsは種別_対象種別_対象_理由が一致するものを除外する() {
        let base_finding = Finding {
            kind: FindingKind::Drift,
            target_kind: crate::model::TargetKind::Edge,
            target: "doc-req:DOC-A:FR-001".to_string(),
            reason: ReasonCode::DocChanged,
            detail: "旧い説明文".to_string(),
        };
        let mut current_same = base_finding.clone();
        current_same.detail = "新しい説明文".to_string(); // 説明文の違いは同一性に影響しない

        let new_finding = Finding {
            kind: FindingKind::New,
            target_kind: crate::model::TargetKind::Edge,
            target: "doc-req:DOC-B:FR-002".to_string(),
            reason: ReasonCode::NewEdge,
            detail: String::new(),
        };

        let current = vec![current_same, new_finding.clone()];
        let base = vec![base_finding];
        let diff = diff_findings(&current, &base);
        assert_eq!(diff, vec![new_finding]);
    }
}
