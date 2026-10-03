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
    use crate::lock::{Lock, LockEntry};

    match target {
        ReconcileTarget::All => {
            if !graph.invalid.is_empty() || !graph.broken.is_empty() {
                return Err(ReconcileError::GraphNotClean);
            }
            let mut new_lock = Lock::default();
            for (edge_id, edge) in &graph.edges {
                let (from_hash, to_hash) = crate::model::edge_hashes(graph, edge);
                let entry = match lock.edges.get(edge_id) {
                    Some(existing) if existing.from == from_hash && existing.to == to_hash => {
                        existing.clone()
                    }
                    _ => LockEntry {
                        from: from_hash,
                        to: to_hash,
                        by: "human".to_string(),
                        reason: reason.clone(),
                        score: None,
                        at: now.to_string(),
                    },
                };
                new_lock.edges.insert(edge_id.clone(), entry);
            }
            Ok(new_lock)
        }
        ReconcileTarget::Edges(ids) => {
            let mut new_lock = lock.clone();
            for id in ids {
                if let Some(edge) = graph.edges.get(&id) {
                    let blocked = graph
                        .invalid
                        .iter()
                        .any(|inv| inv.target == edge.from.id || inv.target == edge.to.id)
                        || graph.broken.iter().any(|b| b.from == edge.from);
                    if blocked {
                        return Err(ReconcileError::Blocked(id));
                    }
                    let (from_hash, to_hash) = crate::model::edge_hashes(graph, edge);
                    new_lock.edges.insert(
                        id,
                        LockEntry {
                            from: from_hash,
                            to: to_hash,
                            by: "human".to_string(),
                            reason: reason.clone(),
                            score: None,
                            at: now.to_string(),
                        },
                    );
                } else if new_lock.edges.contains_key(&id) {
                    // The edge only exists in the lock (it was removed from
                    // the source). Deleting it is normally fine, but if its
                    // endpoints are currently invalid/broken (e.g. the req it
                    // pointed to now has a duplicate id, or the test file it
                    // lived in now fails to parse), removing the record could
                    // hide that problem, so reject it instead.
                    if let Some((kind_prefix, from_id, to_id)) = parse_edge_id(&id) {
                        let mut blocked = graph
                            .invalid
                            .iter()
                            .any(|inv| inv.target == from_id || inv.target == to_id)
                            || graph.broken.iter().any(|b| b.from.id == from_id);
                        if !blocked && kind_prefix == "test-req" {
                            blocked = test_source_has_parse_error(&graph.invalid, from_id);
                        }
                        if blocked {
                            return Err(ReconcileError::Blocked(id));
                        }
                    }
                    new_lock.edges.remove(&id);
                } else {
                    return Err(ReconcileError::UnknownEdge(id));
                }
            }
            Ok(new_lock)
        }
    }
}

/// Splits an edge id (`"doc-req:<from>:<to>"` / `"test-req:<from>:<to>"` /
/// `"desc:doc:<id>"` / `"desc:req:<id>"`) into `(kind_prefix, from_id, to_id)`.
/// A `desc` edge yields `("desc", <id>, <id>)` (a self-loop; the node kind is
/// only part of the edge id). For the other kinds, `to` (a req id) never contains `:`
/// (doc/req ids are restricted to `[A-Za-z0-9._-]`), so it is always the
/// last `:`-separated segment; `from` is everything between the kind prefix
/// and that segment (and may itself contain `:` for Rust test ids).
fn parse_edge_id(id: &str) -> Option<(&str, &str, &str)> {
    if let Some(node_id) = id
        .strip_prefix("desc:doc:")
        .or_else(|| id.strip_prefix("desc:req:"))
    {
        return Some(("desc", node_id, node_id));
    }
    let (rest, to_id) = id.rsplit_once(':')?;
    let (kind_prefix, from_id) = rest.split_once(':')?;
    Some((kind_prefix, from_id, to_id))
}

/// Whether `invalid` contains a `ParseError` whose target (a file path)
/// looks like the source file that test id `test_id` lives in. The "source
/// prefix" is the part of the test id before its first `:` (e.g. Rust
/// `tests/sample::t` -> `tests/sample`, Go `internal/auth:TestX` ->
/// `internal/auth`, Go at the repo root `.:TestX` -> `.`). The match is
/// deliberately conservative (a false positive only blocks a removal, it
/// never silently drops information):
/// - Rust / C# / Java / Kotlin / TypeScript / JavaScript: the target equals
///   `<prefix>.rs` / `<prefix>.cs` / `<prefix>.java` / `<prefix>.kt` /
///   `<prefix>.<ext>` exactly, `<ext>` being one of
///   `TYPESCRIPT_TEST_EXTENSIONS` (TS / JS prefixes keep `.test` / `.spec`).
/// - Go (non-root): the target starts with `<prefix>/`.
/// - Go (repo root, `prefix == "."`): the target contains no `/`.
fn test_source_has_parse_error(invalid: &[crate::model::Invalid], test_id: &str) -> bool {
    let prefix = test_id.split_once(':').map_or(test_id, |(p, _)| p);
    invalid.iter().any(|inv| {
        if inv.reason != crate::model::ReasonCode::ParseError {
            return false;
        }
        let target = inv.target.as_str();
        if target == format!("{prefix}.rs")
            || target == format!("{prefix}.cs")
            || target == format!("{prefix}.java")
            || target == format!("{prefix}.kt")
            || crate::extract::TYPESCRIPT_TEST_EXTENSIONS
                .iter()
                .any(|ext| target == format!("{prefix}.{ext}"))
        {
            return true;
        }
        if prefix == "." {
            !target.contains('/')
        } else {
            target.starts_with(&format!("{prefix}/"))
        }
    })
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
            description: None,
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

    // [FR-035]
    #[test]
    fn ロックにのみ存在するエッジの終端がinvalidな場合はblockedとして拒否される() {
        let graph = {
            let mut graph = Graph::default(); // FR-001 は重複idによりグラフから消えている
            graph.invalid.push(Invalid {
                target_kind: TargetKind::Req,
                target: "FR-001".to_string(),
                reason: ReasonCode::DuplicateId,
                detail: "duplicate id".to_string(),
            });
            graph
        };
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-A:FR-001".to_string(),
            lock_entry("sha256:old-doc", "sha256:old-req", "2025-01-01T00:00:00Z"),
        );
        let before = lock.clone();

        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["doc-req:DOC-A:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
        assert_eq!(lock, before);
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するrustのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        let mut graph = Graph::default(); // テストは構文エラーによりグラフから消えている
        graph.invalid.push(Invalid {
            target_kind: TargetKind::File,
            target: "tests/sample.rs".to_string(),
            reason: ReasonCode::ParseError,
            detail: "parse error".to_string(),
        });
        let mut lock = Lock::default();
        lock.edges.insert(
            "test-req:tests/sample::t:FR-001".to_string(),
            lock_entry("sha256:old-test", "sha256:old-req", "2025-01-01T00:00:00Z"),
        );
        let before = lock.clone();

        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["test-req:tests/sample::t:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
        assert_eq!(lock, before);
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するgoのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        let mut graph = Graph::default(); // テストは構文エラーによりグラフから消えている
        graph.invalid.push(Invalid {
            target_kind: TargetKind::File,
            target: "internal/auth/login_test.go".to_string(),
            reason: ReasonCode::ParseError,
            detail: "parse error".to_string(),
        });
        let mut lock = Lock::default();
        lock.edges.insert(
            "test-req:internal/auth:TestX:FR-001".to_string(),
            lock_entry("sha256:old-test", "sha256:old-req", "2025-01-01T00:00:00Z"),
        );
        let before = lock.clone();

        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["test-req:internal/auth:TestX:FR-001".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
        assert_eq!(lock, before);
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

    fn assert_blocked_by_parse_error(file: &str, edge: &str) {
        let mut graph = Graph::default();
        graph.invalid.push(Invalid {
            target_kind: TargetKind::File,
            target: file.to_string(),
            reason: ReasonCode::ParseError,
            detail: "parse error".to_string(),
        });
        let mut lock = Lock::default();
        lock.edges.insert(
            edge.to_string(),
            lock_entry("sha256:old-test", "sha256:old-req", "2025-01-01T00:00:00Z"),
        );
        let before = lock.clone();

        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec![edge.to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))), "{edge}");
        assert_eq!(lock, before);
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するcsharpのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        assert_blocked_by_parse_error(
            "tests/AuthTests.cs",
            "test-req:tests/AuthTests:AuthTests:T:FR-001",
        );
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するjavaのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        assert_blocked_by_parse_error(
            "tests/AuthTests.java",
            "test-req:tests/AuthTests:AuthTests:t:FR-001",
        );
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するkotlinのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        assert_blocked_by_parse_error(
            "tests/AuthTest.kt",
            "test-req:tests/AuthTest:AuthTest:一覧 取得:FR-001",
        );
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するtypescriptのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        assert_blocked_by_parse_error("src/a.test.ts", "test-req:src/a.test:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.spec.ts", "test-req:src/a.spec:d:t:FR-001");
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するtsxのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        assert_blocked_by_parse_error("src/a.test.tsx", "test-req:src/a.test:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.spec.tsx", "test-req:src/a.spec::t:FR-001");
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するjsのtest_reqエッジは所在ファイルが構文エラーだとblockedになる() {
        assert_blocked_by_parse_error("src/a.test.js", "test-req:src/a.test:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.test.mjs", "test-req:src/a.test:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.spec.cjs", "test-req:src/a.spec:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.spec.mts", "test-req:src/a.spec:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.test.cts", "test-req:src/a.test:d:t:FR-001");
        assert_blocked_by_parse_error("src/a.spec.jsx", "test-req:src/a.spec:d:t:FR-001");
    }

    /// `clean_graph` plus `desc:doc:DOC-A` for the doc (with a description).
    fn desc_graph() -> Graph {
        let mut graph = clean_graph();
        let key = NodeKey {
            kind: NodeKind::Doc,
            id: "DOC-A".to_string(),
        };
        graph.nodes.get_mut(&key).unwrap().description = Some("要約A".to_string());
        graph.edges.insert(
            "desc:doc:DOC-A".to_string(),
            Edge {
                id: "desc:doc:DOC-A".to_string(),
                kind: EdgeKind::Desc,
                from: key.clone(),
                to: key,
            },
        );
        graph
    }

    // [FR-033]
    #[test]
    fn descエッジを指定すると現在の要約と本文のハッシュが記録される() {
        let graph = desc_graph();
        let result = reconcile(
            &graph,
            &Lock::default(),
            ReconcileTarget::Edges(vec!["desc:doc:DOC-A".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let entry = result.edges.get("desc:doc:DOC-A").expect("recorded");
        assert_eq!(entry.from, crate::hash::content_hash("要約A"));
        assert_eq!(entry.to, "sha256:doc-current");
        assert_eq!(entry.by, "human");
        assert!(!result.edges.contains_key("doc-req:DOC-A:FR-001"));
    }

    // [FR-087]
    #[test]
    fn descエッジは要約が無ければ空文字列のハッシュを記録する() {
        let mut graph = desc_graph();
        let key = NodeKey {
            kind: NodeKind::Doc,
            id: "DOC-A".to_string(),
        };
        graph.nodes.get_mut(&key).unwrap().description = None;
        let result = reconcile(
            &graph,
            &Lock::default(),
            ReconcileTarget::Edges(vec!["desc:doc:DOC-A".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(
            result.edges["desc:doc:DOC-A"].from,
            crate::hash::content_hash("")
        );
    }

    // [FR-036]
    #[test]
    fn allはdescエッジも記録し変更のないdescは据え置く() {
        let graph = desc_graph();
        let mut lock = Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-A".to_string(),
            lock_entry(
                &crate::hash::content_hash("要約A"),
                "sha256:doc-current",
                "2025-01-01T00:00:00Z",
            ),
        );
        let before = lock.edges["desc:doc:DOC-A"].clone();
        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::All,
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(result.edges.contains_key("doc-req:DOC-A:FR-001"));
        assert_eq!(result.edges["desc:doc:DOC-A"], before);
    }

    // [FR-036]
    #[test]
    fn allはロックにないdescエッジを新規に記録する() {
        let result = reconcile(
            &desc_graph(),
            &Lock::default(),
            ReconcileTarget::All,
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        let entry = result.edges.get("desc:doc:DOC-A").expect("desc recorded");
        assert_eq!(entry.from, crate::hash::content_hash("要約A"));
        assert_eq!(entry.to, "sha256:doc-current");
        assert_eq!(entry.at, "2026-01-01T00:00:00Z");
    }

    // [FR-034]
    #[test]
    fn ロックにのみ存在するdescエッジを指定するとロックから削除される() {
        let mut lock = Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-GONE".to_string(),
            lock_entry("sha256:old", "sha256:old", "2025-01-01T00:00:00Z"),
        );
        let result = reconcile(
            &Graph::default(),
            &lock,
            ReconcileTarget::Edges(vec!["desc:doc:DOC-GONE".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(!result.edges.contains_key("desc:doc:DOC-GONE"));
    }

    // [FR-035]
    #[test]
    fn ロックにのみ存在するdescエッジのノードがinvalidならblockedになる() {
        let mut graph = Graph::default();
        graph.invalid.push(Invalid {
            target_kind: TargetKind::Doc,
            target: "DOC-A".to_string(),
            reason: ReasonCode::DuplicateId,
            detail: "duplicate id".to_string(),
        });
        let mut lock = Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-A".to_string(),
            lock_entry("sha256:old", "sha256:old", "2025-01-01T00:00:00Z"),
        );
        let before = lock.clone();
        let result = reconcile(
            &graph,
            &lock,
            ReconcileTarget::Edges(vec!["desc:doc:DOC-A".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
        assert_eq!(lock, before);
    }

    // [FR-035]
    #[test]
    fn invalidなdocのdescエッジを指定するとblockedになる() {
        let mut graph = desc_graph();
        graph.invalid.push(Invalid {
            target_kind: TargetKind::Doc,
            target: "DOC-A".to_string(),
            reason: ReasonCode::DuplicateId,
            detail: "duplicate id".to_string(),
        });
        let result = reconcile(
            &graph,
            &Lock::default(),
            ReconcileTarget::Edges(vec!["desc:doc:DOC-A".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
    }

    // [FR-035]
    #[test]
    fn リンク切れを持つdocのdescエッジを指定するとblockedになる() {
        let mut graph = desc_graph();
        graph.broken.push(crate::model::Broken {
            from: NodeKey {
                kind: NodeKind::Doc,
                id: "DOC-A".to_string(),
            },
            target: "../req/missing.md".to_string(),
            reason: ReasonCode::MissingReq,
            detail: "missing".to_string(),
        });
        let result = reconcile(
            &graph,
            &Lock::default(),
            ReconcileTarget::Edges(vec!["desc:doc:DOC-A".to_string()]),
            None,
            "2026-01-01T00:00:00Z",
        );
        assert!(matches!(result, Err(ReconcileError::Blocked(_))));
    }

    // [FR-023]
    #[test]
    fn parse_edge_idはdescエッジを種別とidに分解する() {
        assert_eq!(
            parse_edge_id("desc:doc:DOC-A"),
            Some(("desc", "DOC-A", "DOC-A"))
        );
        assert_eq!(
            parse_edge_id("doc-req:DOC-A:FR-001"),
            Some(("doc-req", "DOC-A", "FR-001"))
        );
    }
}
