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
    Index,
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
    use crate::model::{EdgeKind, NodeKind, ReasonCode, TargetKind};

    let mut findings = Vec::new();

    for (edge_id, edge) in &graph.edges {
        let from_hash = graph
            .nodes
            .get(&edge.from)
            .map(|n| n.hash.clone())
            .unwrap_or_default();
        let to_hash = graph
            .nodes
            .get(&edge.to)
            .map(|n| n.hash.clone())
            .unwrap_or_default();
        match lock.edges.get(edge_id) {
            Some(entry) => {
                let from_changed = entry.from != from_hash;
                let to_changed = entry.to != to_hash;
                if from_changed || to_changed {
                    let reason = if from_changed && to_changed {
                        ReasonCode::BothChanged
                    } else if from_changed {
                        match edge.kind {
                            EdgeKind::DocReq => ReasonCode::DocChanged,
                            EdgeKind::TestReq => ReasonCode::TestChanged,
                        }
                    } else {
                        ReasonCode::ReqChanged
                    };
                    findings.push(Finding {
                        kind: FindingKind::Drift,
                        target_kind: TargetKind::Edge,
                        target: edge_id.clone(),
                        reason,
                        detail: format!("edge {edge_id} drifted from the agreed state"),
                    });
                }
            }
            None => {
                findings.push(Finding {
                    kind: FindingKind::New,
                    target_kind: TargetKind::Edge,
                    target: edge_id.clone(),
                    reason: ReasonCode::NewEdge,
                    detail: format!("edge {edge_id} is not recorded in the lock"),
                });
            }
        }
    }

    for edge_id in lock.edges.keys() {
        if !graph.edges.contains_key(edge_id) {
            findings.push(Finding {
                kind: FindingKind::Removed,
                target_kind: TargetKind::Edge,
                target: edge_id.clone(),
                reason: ReasonCode::RemovedEdge,
                detail: format!("edge {edge_id} no longer exists in the source"),
            });
        }
    }

    // Precompute, in a single pass over the edges, which docs/reqs already
    // have an incoming/outgoing edge, so the orphan/no-test checks below can
    // look each node up in O(1) instead of re-scanning all edges per node.
    let mut docs_with_req: std::collections::HashSet<&crate::model::NodeKey> =
        std::collections::HashSet::new();
    let mut reqs_with_doc: std::collections::HashSet<&crate::model::NodeKey> =
        std::collections::HashSet::new();
    let mut reqs_with_test: std::collections::HashSet<&crate::model::NodeKey> =
        std::collections::HashSet::new();
    for edge in graph.edges.values() {
        match edge.kind {
            EdgeKind::DocReq => {
                docs_with_req.insert(&edge.from);
                reqs_with_doc.insert(&edge.to);
            }
            EdgeKind::TestReq => {
                reqs_with_test.insert(&edge.to);
            }
        }
    }

    for node in graph.nodes.values() {
        match node.key.kind {
            NodeKind::Req => {
                if !reqs_with_doc.contains(&node.key) {
                    findings.push(Finding {
                        kind: FindingKind::Orphan,
                        target_kind: TargetKind::Req,
                        target: node.key.id.clone(),
                        reason: ReasonCode::ReqWithoutDoc,
                        detail: format!("{} is not linked from any doc", node.key.id),
                    });
                }
                if !reqs_with_test.contains(&node.key) {
                    findings.push(Finding {
                        kind: FindingKind::NoTest,
                        target_kind: TargetKind::Req,
                        target: node.key.id.clone(),
                        reason: ReasonCode::ReqWithoutTest,
                        detail: format!("{} has no verifying test", node.key.id),
                    });
                }
            }
            NodeKind::Doc => {
                if !docs_with_req.contains(&node.key) {
                    findings.push(Finding {
                        kind: FindingKind::Orphan,
                        target_kind: TargetKind::Doc,
                        target: node.key.id.clone(),
                        reason: ReasonCode::DocWithoutReq,
                        detail: format!("{} does not link to any req", node.key.id),
                    });
                }
            }
            NodeKind::Test => {}
        }
    }

    for invalid in &graph.invalid {
        findings.push(Finding {
            kind: FindingKind::Invalid,
            target_kind: invalid.target_kind,
            target: invalid.target.clone(),
            reason: invalid.reason,
            detail: invalid.detail.clone(),
        });
    }

    for broken in &graph.broken {
        let target_kind = match broken.from.kind {
            NodeKind::Doc => TargetKind::Doc,
            NodeKind::Test => TargetKind::Test,
            NodeKind::Req => TargetKind::Req,
        };
        findings.push(Finding {
            kind: FindingKind::Broken,
            target_kind,
            target: broken.from.id.clone(),
            reason: broken.reason,
            detail: broken.detail.clone(),
        });
    }

    for issue in &graph.index_issues {
        findings.push(Finding {
            kind: FindingKind::Index,
            target_kind: TargetKind::Index,
            target: issue.target.clone(),
            reason: issue.reason,
            detail: issue.detail.clone(),
        });
    }

    findings.sort();
    findings
}

/// `current - base`, keyed by `(kind, target_kind, target, reason)`.
pub fn diff_findings(current: &[Finding], base: &[Finding]) -> Vec<Finding> {
    let base_keys: std::collections::HashSet<_> = base
        .iter()
        .map(|f| (f.kind, f.target_kind, f.target.clone(), f.reason))
        .collect();
    current
        .iter()
        .filter(|f| !base_keys.contains(&(f.kind, f.target_kind, f.target.clone(), f.reason)))
        .cloned()
        .collect()
}

/// Returns 2 if `gate` and `findings` is non-empty, else 0.
pub fn exit_code(findings: &[Finding], gate: bool) -> i32 {
    if gate && !findings.is_empty() {
        2
    } else {
        0
    }
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
