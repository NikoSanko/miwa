//! Build the doc/req/test dependency graph from a `Source`.

pub fn build_graph(
    source: &dyn crate::source::Source,
    config: &crate::config::Config,
) -> anyhow::Result<crate::model::Graph> {
    use crate::model::{
        edge_id, is_valid_id, Broken, Edge, EdgeKind, Invalid, Node, NodeKey, NodeKind, ReasonCode,
        TargetKind,
    };
    use std::collections::BTreeMap;

    let mut graph = crate::model::Graph::default();
    let files = source.list_files()?;
    let file_set: std::collections::HashSet<&str> = files.iter().map(|s| s.as_str()).collect();

    struct Candidate {
        path: String,
        kind: NodeKind,
        id: Option<String>,
        body: String,
    }
    let mut candidates: Vec<Candidate> = Vec::new();

    for path in &files {
        if !path.ends_with(".md") {
            continue;
        }
        if !(is_under(path, &config.docs) || is_under(path, &config.requirements)) {
            continue;
        }
        let bytes = source.read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let fm = match crate::frontmatter::parse(&text) {
            Ok(Some(fm)) => fm,
            Ok(None) => continue,
            Err(_e) => {
                graph.invalid.push(Invalid {
                    target_kind: TargetKind::File,
                    target: path.clone(),
                    reason: ReasonCode::ParseError,
                    detail: format!("invalid front matter in {path}"),
                });
                continue;
            }
        };
        let kind = match fm.kind.as_deref() {
            Some("Specification") => NodeKind::Doc,
            Some("Requirement") => NodeKind::Req,
            _ => continue,
        };
        candidates.push(Candidate {
            path: path.clone(),
            kind,
            id: fm.id,
            body: fm.body,
        });
    }

    let mut by_kind_id: BTreeMap<(NodeKind, String), Vec<usize>> = BTreeMap::new();
    for (idx, c) in candidates.iter().enumerate() {
        let target_kind = if c.kind == NodeKind::Doc {
            TargetKind::Doc
        } else {
            TargetKind::Req
        };
        let id = match &c.id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => {
                graph.invalid.push(Invalid {
                    target_kind,
                    target: c.path.clone(),
                    reason: ReasonCode::MissingId,
                    detail: format!("{} has no id", c.path),
                });
                continue;
            }
        };
        if !is_valid_id(&id) {
            graph.invalid.push(Invalid {
                target_kind,
                target: id.clone(),
                reason: ReasonCode::InvalidId,
                detail: format!("{} has invalid id {id}", c.path),
            });
            continue;
        }
        by_kind_id.entry((c.kind, id)).or_default().push(idx);
    }

    let mut path_to_req_id: BTreeMap<String, String> = BTreeMap::new();

    for ((kind, id), idxs) in by_kind_id {
        if idxs.len() > 1 {
            let target_kind = if kind == NodeKind::Doc {
                TargetKind::Doc
            } else {
                TargetKind::Req
            };
            for &idx in &idxs {
                graph.invalid.push(Invalid {
                    target_kind,
                    target: id.clone(),
                    reason: ReasonCode::DuplicateId,
                    detail: format!("duplicate id {id} at {}", candidates[idx].path),
                });
            }
            continue;
        }
        let c = &candidates[idxs[0]];
        let key = NodeKey {
            kind,
            id: id.clone(),
        };
        let hash = crate::hash::content_hash(&c.body);
        graph.nodes.insert(
            key.clone(),
            Node {
                key,
                path: c.path.clone(),
                content: c.body.clone(),
                hash,
            },
        );
        if kind == NodeKind::Req {
            path_to_req_id.insert(c.path.clone(), id.clone());
        }
    }

    let doc_entries: Vec<(NodeKey, String, String)> = graph
        .nodes
        .values()
        .filter(|n| n.key.kind == NodeKind::Doc)
        .map(|n| (n.key.clone(), n.path.clone(), n.content.clone()))
        .collect();
    for (doc_key, doc_path, body) in doc_entries {
        let links = crate::markdown::extract_links(&body);
        for link in links {
            let Some(resolved) = crate::markdown::resolve_link(&doc_path, &link) else {
                continue;
            };
            if !is_under(&resolved, &config.requirements) {
                continue;
            }
            if let Some(req_id) = path_to_req_id.get(&resolved) {
                let id = edge_id(EdgeKind::DocReq, &doc_key.id, req_id);
                let req_key = NodeKey {
                    kind: NodeKind::Req,
                    id: req_id.clone(),
                };
                graph.edges.insert(
                    id.clone(),
                    Edge {
                        id,
                        kind: EdgeKind::DocReq,
                        from: doc_key.clone(),
                        to: req_key,
                    },
                );
            } else if file_set.contains(resolved.as_str()) {
                graph.broken.push(Broken {
                    from: doc_key.clone(),
                    target: resolved.clone(),
                    reason: ReasonCode::NotReqFile,
                    detail: format!("{resolved} is not a requirement file"),
                });
            } else {
                graph.broken.push(Broken {
                    from: doc_key.clone(),
                    target: resolved.clone(),
                    reason: ReasonCode::MissingReq,
                    detail: format!("{resolved} does not exist"),
                });
            }
        }
    }

    for path in &files {
        let is_go_test = path.ends_with("_test.go");
        let is_rust = path.ends_with(".rs");
        if !(is_go_test || is_rust) {
            continue;
        }
        if !is_under(path, &config.tests) {
            continue;
        }
        let bytes = source.read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let extracted = if is_go_test {
            crate::extract::extract_go_tests(path, &text)
        } else {
            crate::extract::extract_rust_tests(path, &text)
        };
        let tests = match extracted {
            Ok(tests) => tests,
            Err(_e) => {
                graph.invalid.push(Invalid {
                    target_kind: TargetKind::File,
                    target: path.clone(),
                    reason: ReasonCode::ParseError,
                    detail: format!("failed to parse {path}"),
                });
                continue;
            }
        };
        for t in tests {
            if t.markers.len() > 1 {
                graph.invalid.push(Invalid {
                    target_kind: TargetKind::Test,
                    target: t.id.clone(),
                    reason: ReasonCode::MultipleMarkers,
                    detail: format!("{} has multiple markers: {:?}", t.id, t.markers),
                });
                continue;
            }
            if t.markers.is_empty() {
                continue;
            }
            let req_id = &t.markers[0];
            let test_key = NodeKey {
                kind: NodeKind::Test,
                id: t.id.clone(),
            };
            let hash = crate::hash::content_hash(&t.content);
            graph.nodes.insert(
                test_key.clone(),
                Node {
                    key: test_key.clone(),
                    path: path.clone(),
                    content: t.content.clone(),
                    hash,
                },
            );
            let req_key = NodeKey {
                kind: NodeKind::Req,
                id: req_id.clone(),
            };
            if graph.nodes.contains_key(&req_key) {
                let id = edge_id(EdgeKind::TestReq, &t.id, req_id);
                graph.edges.insert(
                    id.clone(),
                    Edge {
                        id,
                        kind: EdgeKind::TestReq,
                        from: test_key,
                        to: req_key,
                    },
                );
            } else {
                graph.broken.push(Broken {
                    from: test_key,
                    target: req_id.clone(),
                    reason: ReasonCode::MissingReq,
                    detail: format!("{req_id} does not exist"),
                });
            }
        }
    }

    graph.invalid.sort();
    graph.broken.sort();
    Ok(graph)
}

/// Normalizes a configured directory entry: strips a leading `./`, strips a
/// trailing `/`, and maps `.` / `./` / `` (empty) to `.` (the whole repo).
fn normalize_dir(d: &str) -> String {
    let mut s = d.trim();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    let s = s.trim_end_matches('/');
    if s.is_empty() {
        ".".to_string()
    } else {
        s.to_string()
    }
}

fn is_under(path: &str, dirs: &[String]) -> bool {
    dirs.iter().any(|d| {
        let normalized = normalize_dir(d);
        if normalized == "." {
            true
        } else {
            path == normalized.as_str() || path.starts_with(&format!("{normalized}/"))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, JudgeConfig};
    use crate::model::{NodeKind, ReasonCode, TargetKind};
    use crate::source::Source;
    use std::collections::BTreeMap;

    struct FakeSource {
        files: BTreeMap<String, Vec<u8>>,
    }

    impl FakeSource {
        fn new(files: &[(&str, &str)]) -> FakeSource {
            FakeSource {
                files: files
                    .iter()
                    .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
                    .collect(),
            }
        }
    }

    impl Source for FakeSource {
        fn list_files(&self) -> anyhow::Result<Vec<String>> {
            Ok(self.files.keys().cloned().collect())
        }

        fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("not found: {path}"))
        }
    }

    fn config() -> Config {
        Config {
            docs: vec!["docs".to_string()],
            requirements: vec!["req".to_string()],
            tests: vec![".".to_string()],
            diff_base: "main".to_string(),
            judge: JudgeConfig {
                threshold: 0.9,
                max_body_bytes: 200_000,
            },
        }
    }

    // [FR-005]
    #[test]
    fn typeフィールドのないmdファイルはノードとして扱われない() {
        let source = FakeSource::new(&[
            ("docs/a.md", "# no frontmatter\nplain text\n"),
            (
                "req/fr1.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Doc));
        assert!(
            graph.invalid.is_empty(),
            "should be silently excluded, not invalid: {:?}",
            graph.invalid
        );
    }

    // [FR-005]
    #[test]
    fn typeの値がspecification_requirement以外なら対象外になる() {
        let source = FakeSource::new(&[
            ("docs/b.md", "---\ntype: Note\nid: DOC-B\n---\n本文\n"),
            (
                "req/fr1.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(!graph.nodes.values().any(|n| n.key.id == "DOC-B"));
    }

    // [FR-006]
    #[test]
    fn 不正な文字を含むidはinvalid_idとして報告される() {
        let source = FakeSource::new(&[(
            "req/bad.md",
            "---\ntype: Requirement\nid: FR:001\n---\n本文\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::InvalidId && i.target_kind == TargetKind::Req));
    }

    // [FR-007]
    #[test]
    fn idが空のファイルはmissing_idとして報告される() {
        let source = FakeSource::new(&[(
            "req/empty-id.md",
            "---\ntype: Requirement\nid: \n---\n本文\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MissingId));
    }

    // [FR-007]
    #[test]
    fn 同じidを持つファイルが複数あるとduplicate_idとして報告される() {
        let source = FakeSource::new(&[
            (
                "req/fr1.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文A\n",
            ),
            (
                "req/fr1-dup.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文B\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::DuplicateId && i.target == "FR-001"));
    }

    // [FR-010]
    #[test]
    fn requirements領域外を指すリンクはエッジにもbrokenにもならない() {
        let source = FakeSource::new(&[
            (
                "docs/a.md",
                "---\ntype: Specification\nid: DOC-A\n---\n[外部req](../outside/x.md)\n",
            ),
            (
                "outside/x.md",
                "---\ntype: Requirement\nid: FR-999\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .edges
            .values()
            .all(|e| e.kind != crate::model::EdgeKind::DocReq));
        assert!(
            graph.broken.is_empty(),
            "should not be reported as broken: {:?}",
            graph.broken
        );
    }

    // [FR-011]
    #[test]
    fn 存在しないreqファイルを指すリンクはbrokenになる() {
        let source = FakeSource::new(&[(
            "docs/a.md",
            "---\ntype: Specification\nid: DOC-A\n---\n[リンク](../req/missing.md)\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .broken
            .iter()
            .any(|b| b.from.kind == NodeKind::Doc && b.reason == ReasonCode::MissingReq));
    }

    // [FR-011]
    #[test]
    fn requirement型でないファイルを指すリンクはnot_req_fileとしてbrokenになる() {
        let source = FakeSource::new(&[
            (
                "docs/a.md",
                "---\ntype: Specification\nid: DOC-A\n---\n[リンク](../req/not-a-req.md)\n",
            ),
            (
                "req/not-a-req.md",
                "---\ntype: Specification\nid: DOC-FAKE\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .broken
            .iter()
            .any(|b| b.from.kind == NodeKind::Doc && b.reason == ReasonCode::NotReqFile));
    }

    // [FR-011]
    #[test]
    fn 存在しないreq_idを指すtestマーカーはbrokenになる() {
        let source = FakeSource::new(&[(
            "src/foo.rs",
            "// [FR-999]\n#[test]\nfn does_something() {}\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .broken
            .iter()
            .any(|b| b.from.kind == NodeKind::Test && b.reason == ReasonCode::MissingReq));
    }

    // [FR-014]
    #[test]
    fn go_1つのテストに2件以上のマーカーがあるとinvalidになる() {
        let source = FakeSource::new(&[(
            "main_test.go",
            "package main\n\n// [FR-001]\n// [FR-002]\nfunc TestBoth(t *testing.T) {}\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MultipleMarkers));
    }

    // [FR-014]
    #[test]
    fn 構文エラーのrustテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[("broken.rs", "fn totally #%^& invalid syntax {\n")]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("broken.rs")));
    }

    // [FR-014]
    #[test]
    fn rust_1つのテストに2件以上のマーカーがあるとinvalidになる() {
        let source = FakeSource::new(&[(
            "src/foo.rs",
            "// [FR-001]\n// [FR-002]\n#[test]\nfn does_both() {}\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MultipleMarkers));
    }

    // [FR-014]
    #[test]
    fn 構文エラーのgoテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[("broken_test.go", "package main\n\nfunc TestBroken( {\n")]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("broken_test.go")));
    }

    // [FR-018]
    #[test]
    fn configのディレクトリ表記がドットスラッシュや末尾スラッシュ付きでも拾われる() {
        let source = FakeSource::new(&[
            (
                "docs/spec.md",
                "---\ntype: Specification\nid: DOC-A\n---\n[リンク](../req/fr1.md)\n",
            ),
            (
                "req/fr1.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文\n",
            ),
        ]);
        let config = Config {
            docs: vec!["./docs".to_string()],
            requirements: vec!["req/".to_string()],
            tests: vec![".".to_string()],
            diff_base: "main".to_string(),
            judge: JudgeConfig {
                threshold: 0.9,
                max_body_bytes: 200_000,
            },
        };
        let graph = build_graph(&source, &config).unwrap();
        assert!(graph
            .nodes
            .keys()
            .any(|k| k.kind == NodeKind::Doc && k.id == "DOC-A"));
        assert!(graph
            .nodes
            .keys()
            .any(|k| k.kind == NodeKind::Req && k.id == "FR-001"));
        assert!(graph.edges.contains_key("doc-req:DOC-A:FR-001"));
    }
}
