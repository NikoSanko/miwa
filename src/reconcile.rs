//! `reconcile`: record the current state of one or more edges into the lock.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileTarget {
    All,
    Edges(Vec<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("unknown edge: {0}")]
    UnknownEdge(String),
    #[error("edge {0} has structural problems")]
    Blocked(String),
    #[error("graph has broken references or invalid nodes")]
    GraphNotClean,
}

pub fn reconcile(
    graph: &crate::model::Graph,
    lock: &crate::lock::Lock,
    target: ReconcileTarget,
    reason: Option<String>,
    now: &str,
) -> Result<crate::lock::Lock, ReconcileError> {
    let _ = (graph, lock, target, reason, now);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::{Lock, LockEntry};
    use crate::model::{
        Edge, EdgeKind, Graph, Invalid, Node, NodeKey, NodeKind, ReasonCode, TargetKind,
    };

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

    fn lock_entry(from: &str, to: &str, at: &str) -> LockEntry {
        LockEntry {
            from: from.to_string(),
            to: to.to_string(),
            by: "human".to_string(),
            reason: None,
            score: None,
            at: at.to_string(),
        }
    }

    /// A clean graph with a single doc-req edge, no broken/invalid entries.
    fn clean_graph() -> Graph {
        let mut graph = Graph::default();
        let doc_key = NodeKey {
            kind: NodeKind::Doc,
            id: "DOC-A".to_string(),
        };
        let req_key = NodeKey {
            kind: NodeKind::Req,
            id: "FR-001".to_string(),
        };
        graph.nodes.insert(
            doc_key.clone(),
            node(NodeKind::Doc, "DOC-A", "sha256:doc-current"),
        );
        graph.nodes.insert(
            req_key.clone(),
            node(NodeKind::Req, "FR-001", "sha256:req-current"),
        );
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

    // [FR-033]
    #[test]
    fn edgeを指定すると現在の両端のハッシュがロックに記録される() {
        let graph = clean_graph();
        let lock = Lock::default();
        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["doc-req:DOC-A:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let entry = result
            .edges
            .get("doc-req:DOC-A:FR-001")
            .expect("edge should be recorded");
        assert_eq!(entry.from, "sha256:doc-current");
        assert_eq!(entry.to, "sha256:req-current");
        assert_eq!(entry.by, "human");
    }

    // [FR-034]
    #[test]
    fn ロックにのみ存在するエッジを指定するとロックから削除される() {
        let graph = Graph::default(); // グラフには存在しない
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry("sha256:old-doc", "sha256:old-req", "2025-01-01T00:00:00Z"),
        );
        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["doc-req:DOC-A:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(!result.edges.contains_key("doc-req:DOC-A:FR-001"));
    }

    // [FR-034]
    #[test]
    fn ロックにもグラフにも存在しないエッジを指定するとエラーになる() {
        let graph = Graph::default();
        let lock = Lock::default();
        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["doc-req:NOWHERE:FR-999".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::UnknownEdge(_))));
    }

    // [FR-035]
    #[test]
    fn 対象reqがinvalidに関わるエッジはreconcileを拒否される() {
        let mut graph = clean_graph();
        // FR-001 は id が重複しており invalid として報告されている
        graph.invalid.push(Invalid {
            target_kind: TargetKind::Req,
            target: "FR-001".to_string(),
            reason: ReasonCode::DuplicateId,
            detail: "duplicate id".to_string(),
        });
        let lock = Lock::default();
        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["doc-req:DOC-A:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
        // ロックは変更されていないはず（今回は空のロックのまま）
    }

    // [FR-036]
    #[test]
    fn allは変更のないエッジを据え置き_新規を追加し_消えたエッジを削除する() {
        let mut graph = clean_graph();
        // 現在のグラフに存在する新規エッジ（reqにのみ紐づくtest-reqエッジ）
        let test_key = NodeKey {
            kind: NodeKind::Test,
            id: "src/foo::bar".to_string(),
        };
        graph.nodes.insert(
            test_key.clone(),
            node(NodeKind::Test, "src/foo::bar", "sha256:test-current"),
        );
        let req_key = NodeKey {
            kind: NodeKind::Req,
            id: "FR-001".to_string(),
        };
        let new_edge_id = "test-req:src/foo::bar:FR-001".to_string();
        graph.edges.insert(
            new_edge_id.clone(),
            Edge {
                id: new_edge_id.clone(),
                kind: EdgeKind::TestReq,
                from: test_key,
                to: req_key,
            },
        );

        let mut lock = Lock::default();
        // 既に合意済みで、ハッシュが変化していないエッジ（据え置かれるはず）
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry(
                "sha256:doc-current",
                "sha256:req-current",
                "2020-01-01T00:00:00Z",
            ),
        );
        // ソースからは既に消えたエッジ（削除されるはず）
        lock.edges.insert(
            "doc-req:DOC-STALE:FR-999".to_string(),
            lock_entry(
                "sha256:stale-doc",
                "sha256:stale-req",
                "2020-01-01T00:00:00Z",
            ),
        );

        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::All,
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        // 据え置き: 変更なしのエッジは元のエントリのまま（at が上書きされない）
        let kept = result.edges.get("doc-req:DOC-A:FR-001").unwrap();
        assert_eq!(kept.at, "2020-01-01T00:00:00Z");

        // 削除: ソースから消えたエッジはロックに残らない
        assert!(!result.edges.contains_key("doc-req:DOC-STALE:FR-999"));

        // 追加: 新規エッジが記録される
        let added = result.edges.get("test-req:src/foo::bar:FR-001").unwrap();
        assert_eq!(added.from, "sha256:test-current");
        assert_eq!(added.to, "sha256:req-current");
        assert_eq!(added.by, "human");
    }

    // [FR-036]
    #[test]
    fn 対象にbrokenまたはinvalidが1件でもあればallは全く変更せず失敗する() {
        let mut graph = clean_graph();
        graph.invalid.push(Invalid {
            target_kind: TargetKind::Req,
            target: "FR-999".to_string(),
            reason: ReasonCode::MissingId,
            detail: "missing id".to_string(),
        });
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry(
                "sha256:doc-current",
                "sha256:req-current",
                "2020-01-01T00:00:00Z",
            ),
        );
        let before = lock.clone();

        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::All,
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::GraphNotClean)));
        // 呼び出し元のロックは reconcile によって書き換えられていない（純粋関数なので当然だが明示する）
        assert_eq!(lock, before);
    }

    // [FR-037]
    #[test]
    fn reasonを指定した場合はエントリに記録される() {
        let graph = clean_graph();
        let result = reconcile(
            &graph,
            &Lock::default(),
            ReconcileTarget::Edges(vec!["doc-req:DOC-A:FR-001".to_string()]),
            Some("最小長の記述が一致".to_string()),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let entry = result.edges.get("doc-req:DOC-A:FR-001").unwrap();
        assert_eq!(entry.by, "human");
        assert_eq!(entry.reason, Some("最小長の記述が一致".to_string()));
    }

    // [FR-037]
    #[test]
    fn reasonを指定しない場合は記録されない() {
        let graph = clean_graph();
        let result = reconcile(
            &graph,
            &Lock::default(),
            ReconcileTarget::Edges(vec!["doc-req:DOC-A:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let entry = result.edges.get("doc-req:DOC-A:FR-001").unwrap();
        assert_eq!(entry.by, "human");
        assert_eq!(entry.reason, None);
    }
}
