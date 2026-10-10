//! Build the doc/req/test dependency graph from a `Source`.

pub fn build_graph(
    source: &dyn crate::source::Source,
    config: &crate::config::Config,
) -> anyhow::Result<crate::model::Graph> {
    use crate::model::{
        desc_edge_id, edge_id, is_valid_id, Broken, Edge, EdgeKind, Invalid, Node, NodeKey,
        NodeKind, ReasonCode, TargetKind,
    };
    use std::collections::{BTreeMap, BTreeSet};

    let mut graph = crate::model::Graph::default();
    let files: Vec<String> = source
        .list_files()?
        .into_iter()
        .filter(|path| !crate::source::is_under_dir_named(path, &config.exclude))
        .collect();
    let file_set: std::collections::HashSet<&str> = files.iter().map(|s| s.as_str()).collect();

    struct Candidate {
        path: String,
        kind: NodeKind,
        id: Option<String>,
        body: String,
        description: Option<String>,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut texts: BTreeMap<String, crate::index::ParsedText> = BTreeMap::new();
    let index_roots: Vec<String> = config
        .index
        .iter()
        .map(|r| crate::config::normalize_dir(r))
        .collect();

    for path in &files {
        if !path.ends_with(".md") {
            continue;
        }
        if !(is_under_any(path, &config.docs) || is_under_any(path, &config.requirements)) {
            continue;
        }
        let bytes = source.read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let parsed = crate::frontmatter::parse(&text);
        // Reserved names never become nodes (the index still reads their bodies).
        if !matches!(path.rsplit('/').next(), Some("index.md" | "log.md")) {
            match &parsed {
                Ok(Some(fm)) => {
                    let kind = match fm.kind.as_deref() {
                        Some("BusinessNeed") => Some(NodeKind::Doc),
                        Some("Requirement") => Some(NodeKind::Req),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        candidates.push(Candidate {
                            path: path.clone(),
                            kind,
                            id: fm.id.clone(),
                            body: fm.body.clone(),
                            description: crate::index::normalize_description(fm),
                        });
                    }
                }
                Ok(None) => {}
                Err(_e) => {
                    graph.invalid.push(Invalid {
                        target_kind: TargetKind::File,
                        target: path.clone(),
                        reason: ReasonCode::ParseError,
                        detail: format!("invalid front matter in {path}"),
                    });
                }
            }
        }
        // Bodies are only kept for files under an index root.
        if index_roots.iter().any(|r| crate::config::is_under(path, r)) {
            texts.insert(
                path.clone(),
                crate::index::ParsedText {
                    text,
                    front_matter: parsed,
                },
            );
        }
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
                description: c.description.clone(),
            },
        );
        if kind == NodeKind::Req {
            path_to_req_id.insert(c.path.clone(), id.clone());
        }
    }

    // `desc:doc:<id>` / `desc:req:<id>` self-loop edges for Doc / Req nodes under an index root.
    let desc_edges: Vec<Edge> = graph
        .nodes
        .values()
        .filter(|n| matches!(n.key.kind, NodeKind::Doc | NodeKind::Req))
        .filter(|n| {
            index_roots
                .iter()
                .any(|r| crate::config::is_under(&n.path, r))
        })
        .map(|n| Edge {
            id: desc_edge_id(&n.key),
            kind: EdgeKind::Desc,
            from: n.key.clone(),
            to: n.key.clone(),
        })
        .collect();
    for e in desc_edges {
        graph.edges.insert(e.id.clone(), e);
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
            if !is_under_any(&resolved, &config.requirements) {
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

    let mut extracted_tests: Vec<(String, crate::extract::ExtractedTest)> = Vec::new();
    for path in &files {
        if !is_under_any(path, &config.tests) {
            continue;
        }
        let Some(language) = test_language(path) else {
            continue;
        };
        let bytes = source.read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let result = match language {
            TestLanguage::Go => crate::extract::extract_go_tests(path, &text),
            TestLanguage::Rust => {
                crate::extract::extract_rust_tests(path, &text, &config.rust.test_attributes)
            }
            TestLanguage::CSharp => crate::extract::extract_csharp_tests(path, &text),
            TestLanguage::Java => crate::extract::extract_java_tests(path, &text),
            TestLanguage::Kotlin => crate::extract::extract_kotlin_tests(path, &text),
            TestLanguage::TypeScript => crate::extract::extract_typescript_tests(path, &text),
        };
        match result {
            Ok(tests) => {
                extracted_tests.extend(
                    tests
                        .into_iter()
                        .filter(|t| !t.markers.is_empty())
                        .map(|t| (path.clone(), t)),
                );
            }
            Err(_e) => {
                graph.invalid.push(Invalid {
                    target_kind: TargetKind::File,
                    target: path.clone(),
                    reason: ReasonCode::ParseError,
                    detail: format!("failed to parse {path}"),
                });
            }
        }
    }

    // Group marked tests by identifier. Occurrences with several markers are
    // invalid and excluded; the rest are merged into one test when they all
    // point at the same req (FR-057).
    let mut by_test_id: BTreeMap<String, Vec<(String, crate::extract::ExtractedTest)>> =
        BTreeMap::new();
    for (path, t) in extracted_tests {
        if t.markers.len() > 1 {
            graph.invalid.push(Invalid {
                target_kind: TargetKind::Test,
                target: t.id.clone(),
                reason: ReasonCode::MultipleMarkers,
                detail: format!("{} has multiple markers: {:?}", t.id, t.markers),
            });
            continue;
        }
        by_test_id.entry(t.id.clone()).or_default().push((path, t));
    }

    for (test_id, occurrences) in by_test_id {
        let req_ids: std::collections::BTreeSet<&String> =
            occurrences.iter().map(|(_, t)| &t.markers[0]).collect();
        if req_ids.len() > 1 {
            let paths: BTreeSet<&str> = occurrences.iter().map(|(p, _)| p.as_str()).collect();
            let reqs: Vec<&str> = req_ids.iter().map(|r| r.as_str()).collect();
            graph.invalid.push(Invalid {
                target_kind: TargetKind::Test,
                target: test_id.clone(),
                reason: ReasonCode::DuplicateId,
                detail: format!(
                    "duplicate test id {test_id} points at different requirements ({}) at {}",
                    reqs.join(", "),
                    paths.into_iter().collect::<Vec<_>>().join(", ")
                ),
            });
            continue;
        }
        let req_id = occurrences[0].1.markers[0].clone();
        let path = occurrences[0].0.clone();
        let content = occurrences
            .iter()
            .map(|(_, t)| t.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let test_key = NodeKey {
            kind: NodeKind::Test,
            id: test_id.clone(),
        };
        let hash = crate::hash::content_hash(&content);
        graph.nodes.insert(
            test_key.clone(),
            Node {
                key: test_key.clone(),
                path,
                content,
                hash,
                description: None,
            },
        );
        let req_key = NodeKey {
            kind: NodeKind::Req,
            id: req_id.clone(),
        };
        if graph.nodes.contains_key(&req_key) {
            let id = edge_id(EdgeKind::TestReq, &test_id, &req_id);
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

    // Index checks only run when at least one index root is configured.
    if !config.index.is_empty() {
        graph.index_issues = crate::index::collect_issues(config, &files, &texts)
            .into_iter()
            .collect();
    }

    graph.invalid.sort();
    graph.broken.sort();
    Ok(graph)
}

fn is_under_any(path: &str, dirs: &[String]) -> bool {
    dirs.iter()
        .any(|d| crate::config::is_under(path, &crate::config::normalize_dir(d)))
}

#[derive(Debug, Clone, Copy)]
enum TestLanguage {
    Go,
    Rust,
    CSharp,
    Java,
    Kotlin,
    TypeScript,
}

/// Decides the extraction language from the file path; `None` if the file is
/// not a test file of a supported language.
fn test_language(path: &str) -> Option<TestLanguage> {
    if path.ends_with("_test.go") {
        Some(TestLanguage::Go)
    } else if path.ends_with(".rs") {
        Some(TestLanguage::Rust)
    } else if path.ends_with(".cs") {
        Some(TestLanguage::CSharp)
    } else if path.ends_with(".java") {
        Some(TestLanguage::Java)
    } else if path.ends_with(".kt") {
        Some(TestLanguage::Kotlin)
    } else if crate::extract::is_typescript_test_file(path) {
        Some(TestLanguage::TypeScript)
    } else {
        None
    }
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
            rust: crate::config::RustConfig::default(),
            exclude: vec![],
            index: vec![],
            site: None,
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
    fn typeの値がbusiness_need_requirement以外なら対象外になる() {
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

    // [FR-005]
    #[test]
    fn type_specificationの文書はdocノードにならない() {
        let source = FakeSource::new(&[
            (
                "docs/old.md",
                "---\ntype: Specification\nid: DOC-OLD\n---\n本文\n",
            ),
            (
                "req/fr1.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(!graph.nodes.values().any(|n| n.key.id == "DOC-OLD"));
        assert!(
            graph.invalid.is_empty(),
            "should be silently excluded, not invalid: {:?}",
            graph.invalid
        );
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
                "---\ntype: BusinessNeed\nid: DOC-A\n---\n[外部req](../outside/x.md)\n",
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
            "---\ntype: BusinessNeed\nid: DOC-A\n---\n[リンク](../req/missing.md)\n",
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
                "---\ntype: BusinessNeed\nid: DOC-A\n---\n[リンク](../req/not-a-req.md)\n",
            ),
            (
                "req/not-a-req.md",
                "---\ntype: BusinessNeed\nid: DOC-FAKE\n---\n本文\n",
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
                "---\ntype: BusinessNeed\nid: DOC-A\n---\n[リンク](../req/fr1.md)\n",
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
            rust: crate::config::RustConfig::default(),
            exclude: vec![],
            index: vec![],
            site: None,
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

    const REQ1: (&str, &str) = (
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文\n",
    );

    fn has_test_node(graph: &crate::model::Graph, id: &str) -> bool {
        graph
            .nodes
            .keys()
            .any(|k| k.kind == NodeKind::Test && k.id == id)
    }

    // [FR-049]
    #[test]
    fn csharpファイルからtestノードとエッジが作られる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(has_test_node(&graph, "tests/AuthTests:AuthTests:T"));
        assert!(graph
            .edges
            .contains_key("test-req:tests/AuthTests:AuthTests:T:FR-001"));
    }

    // [FR-015]
    #[test]
    fn configのrust_test_attributesが抽出に反映される() {
        let src = "// [FR-001]\n#[rstest]\nfn t() {}\n";
        let source = FakeSource::new(&[REQ1, ("src/a.rs", src)]);

        let graph = build_graph(&source, &config()).unwrap();
        assert!(!has_test_node(&graph, "src/a::t"));

        let mut cfg = config();
        cfg.rust.test_attributes = vec!["rstest".to_string()];
        let graph = build_graph(&source, &cfg).unwrap();
        assert!(has_test_node(&graph, "src/a::t"));
        assert!(graph.edges.contains_key("test-req:src/a::t:FR-001"));
    }

    // [FR-053]
    #[test]
    fn typescriptのtest_tsとspec_tsxからtestノードが作られる() {
        let source = FakeSource::new(&[
            REQ1,
            ("src/a.test.ts", "// [FR-001]\nit(\"works\", () => {});\n"),
            (
                "src/b.spec.tsx",
                "// [FR-001]\nit(\"renders\", () => { const x = <div />; });\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(has_test_node(&graph, "src/a.test::works"));
        assert!(has_test_node(&graph, "src/b.spec::renders"));
    }

    // [FR-053]
    #[test]
    fn typescriptのtestやspecを含まない通常のtsファイルは走査されない() {
        let source = FakeSource::new(&[
            REQ1,
            ("src/a.ts", "// [FR-001]\nit(\"works\", () => {});\n"),
            ("src/b.tsx", "// [FR-001]\nit(\"works\", () => {});\n"),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
        assert!(graph.invalid.is_empty());
    }

    // [FR-053]
    #[test]
    fn javascriptのtest_jsとspec_mjsからtestノードが作られる() {
        let source = FakeSource::new(&[
            REQ1,
            ("src/a.test.js", "// [FR-001]\nit(\"works\", () => {});\n"),
            ("src/b.spec.mjs", "// [FR-001]\nit(\"loads\", () => {});\n"),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(has_test_node(&graph, "src/a.test::works"));
        assert!(has_test_node(&graph, "src/b.spec::loads"));
    }

    // [FR-053]
    #[test]
    fn testやspecを含まない通常のjsファイルは走査されない() {
        let source = FakeSource::new(&[
            REQ1,
            ("src/a.js", "// [FR-001]\nit(\"works\", () => {});\n"),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
        assert!(graph.invalid.is_empty());
    }

    // [FR-014]
    #[test]
    fn 構文エラーのjsテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[("src/broken.test.js", "it(\"t\", () => {\n")]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("broken.test.js")));
    }

    const REQ2: (&str, &str) = (
        "req/fr2.md",
        "---\ntype: Requirement\nid: FR-002\n---\n本文2\n",
    );

    fn test_node<'a>(graph: &'a crate::model::Graph, id: &str) -> &'a crate::model::Node {
        graph
            .nodes
            .values()
            .find(|n| n.key.kind == NodeKind::Test && n.key.id == id)
            .unwrap_or_else(|| panic!("test node {id} not found"))
    }

    fn cs_overloads(first: &str, second: &str) -> String {
        format!(
            "public class AuthTests\n{{\n    // [FR-001]\n    [Fact]\n    public void T() {{ {first} }}\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) {{ {second} }}\n}}\n"
        )
    }

    // [FR-057]
    #[test]
    fn csharpのオーバーロードは1つのtestにまとめられcontentが出現順に連結される() {
        let source = FakeSource::new(&[
            REQ1,
            ("tests/AuthTests.cs", &cs_overloads("one();", "two();")),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let tests: Vec<_> = graph
            .nodes
            .keys()
            .filter(|k| k.kind == NodeKind::Test)
            .collect();
        assert_eq!(tests.len(), 1, "{tests:?}");
        let edges: Vec<_> = graph
            .edges
            .values()
            .filter(|e| e.kind == crate::model::EdgeKind::TestReq)
            .collect();
        assert_eq!(edges.len(), 1, "{edges:?}");
        let node = test_node(&graph, "tests/AuthTests:AuthTests:T");
        let a = node.content.find("one();").expect("first occurrence");
        let b = node.content.find("two();").expect("second occurrence");
        assert!(a < b, "{:?}", node.content);
        let between = &node.content[a..b];
        assert!(between.contains('\n'), "{:?}", node.content);
        assert!(node.content.contains("public void T()"));
        assert!(node.content.contains("public void T(int x)"));
    }

    // [FR-057]
    #[test]
    fn goの別ファイルにある同名testはファイルパス順に連結される() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "pkg/b_test.go",
                "package pkg\n\n// [FR-001]\nfunc TestX(t *testing.T) { second_body() }\n",
            ),
            (
                "pkg/a_test.go",
                "package pkg\n\n// [FR-001]\nfunc TestX(t *testing.T) { first_body() }\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let node = test_node(&graph, "pkg:TestX");
        let a = node.content.find("first_body").expect("a_test.go");
        let b = node.content.find("second_body").expect("b_test.go");
        assert!(a < b, "{:?}", node.content);
    }

    // [FR-057]
    #[test]
    fn rustのcfg分岐による同名testは1つのtestにまとめられる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "src/lib.rs",
                "#[cfg(unix)]\n// [FR-001]\n#[test]\nfn same() { unix_body(); }\n\n#[cfg(windows)]\n// [FR-001]\n#[test]\nfn same() { windows_body(); }\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let node = test_node(&graph, "src/lib::same");
        let a = node.content.find("unix_body").expect("unix");
        let b = node.content.find("windows_body").expect("windows");
        assert!(a < b, "{:?}", node.content);
    }

    // [FR-057]
    #[test]
    fn typescriptの同名testは1つのtestにまとめられる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "src/a.test.ts",
                "// [FR-001]\nit(\"same\", () => { first(); });\n\n// [FR-001]\nit(`same`, () => { second(); });\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let node = test_node(&graph, "src/a.test::same");
        assert!(node.content.contains("first();"));
        assert!(node.content.contains("second();"));
    }

    // [FR-057]
    #[test]
    fn まとめたtestは出現の本体が変わるとhashが変わる() {
        let hash_of = |first: &str, second: &str| {
            let source =
                FakeSource::new(&[REQ1, ("tests/AuthTests.cs", &cs_overloads(first, second))]);
            let graph = build_graph(&source, &config()).unwrap();
            test_node(&graph, "tests/AuthTests:AuthTests:T")
                .hash
                .clone()
        };
        let base = hash_of("one();", "two();");
        assert_ne!(base, hash_of("one();", "changed();"));
        assert_ne!(base, hash_of("changed();", "two();"));
    }

    // [FR-057]
    #[test]
    fn まとめたtestは出現の順序を入れ替えるとhashが変わる() {
        let hash_of = |first: &str, second: &str| {
            let src = format!(
                "public class AuthTests\n{{\n    // [FR-001]\n    [Fact]\n    public void T({first}) {{ }}\n\n    // [FR-001]\n    [Fact]\n    public void T({second}) {{ }}\n}}\n"
            );
            let source = FakeSource::new(&[REQ1, ("tests/AuthTests.cs", &src)]);
            let graph = build_graph(&source, &config()).unwrap();
            test_node(&graph, "tests/AuthTests:AuthTests:T")
                .hash
                .clone()
        };
        assert_ne!(hash_of("int x", "string s"), hash_of("string s", "int x"));
    }

    // [FR-057]
    #[test]
    fn 異なるreqを指す重複はduplicate_idとなりノードもエッジも作られない() {
        let source = FakeSource::new(&[
            REQ1,
            REQ2,
            (
                "pkg/a_test.go",
                "package pkg\n\n// [FR-001]\nfunc TestX(t *testing.T) {}\n",
            ),
            (
                "pkg/b_test.go",
                "package pkg\n\n// [FR-002]\nfunc TestX(t *testing.T) {}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        let dups: Vec<_> = graph
            .invalid
            .iter()
            .filter(|i| i.reason == ReasonCode::DuplicateId)
            .collect();
        assert_eq!(dups.len(), 1, "{:?}", graph.invalid);
        assert_eq!(dups[0].target_kind, TargetKind::Test);
        assert_eq!(dups[0].target, "pkg:TestX");
        assert!(dups[0].detail.contains("pkg/a_test.go"));
        assert!(dups[0].detail.contains("pkg/b_test.go"));
        assert!(dups[0].detail.contains("FR-001"));
        assert!(dups[0].detail.contains("FR-002"));
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
        assert!(graph
            .edges
            .values()
            .all(|e| e.kind != crate::model::EdgeKind::TestReq));
        let findings = crate::check::check(&graph, &crate::lock::Lock::default());
        for req in ["FR-001", "FR-002"] {
            assert!(
                findings
                    .iter()
                    .any(|f| f.kind == crate::check::FindingKind::NoTest && f.target == req),
                "{req}: {findings:?}"
            );
        }
    }

    // [FR-057]
    #[test]
    fn 同一ファイル内で異なるreqを指す重複は識別子ごとに1件でファイル名は1回だけ出る() {
        let source = FakeSource::new(&[
            REQ1,
            REQ2,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { }\n\n    // [FR-002]\n    [Fact]\n    public void T(int x) { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        let dups: Vec<_> = graph
            .invalid
            .iter()
            .filter(|i| i.reason == ReasonCode::DuplicateId)
            .collect();
        assert_eq!(dups.len(), 1, "{:?}", graph.invalid);
        assert_eq!(dups[0].detail.matches("tests/AuthTests.cs").count(), 1);
        assert!(dups[0].detail.contains("FR-001"));
        assert!(dups[0].detail.contains("FR-002"));
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
    }

    // [FR-057]
    #[test]
    fn 重複の1つが複数マーカーならその出現はmultiple_markersで残りだけがまとめられる() {
        let source = FakeSource::new(&[
            REQ1,
            REQ2,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    // [FR-002]\n    [Fact]\n    public void T() { multi(); }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { single_a(); }\n\n    // [FR-001]\n    [Fact]\n    public void T(string s) { single_b(); }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(
            graph
                .invalid
                .iter()
                .any(|i| i.reason == ReasonCode::MultipleMarkers),
            "{:?}",
            graph.invalid
        );
        assert!(!graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::DuplicateId));
        let node = test_node(&graph, "tests/AuthTests:AuthTests:T");
        assert!(node.content.contains("single_a();"));
        assert!(node.content.contains("single_b();"));
        assert!(!node.content.contains("multi();"));
    }

    // [FR-057]
    #[test]
    fn 重複の1つが複数マーカーで残りが1件なら通常のtestとして扱われる() {
        let source = FakeSource::new(&[
            REQ1,
            REQ2,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    // [FR-002]\n    [Fact]\n    public void T() { multi(); }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { single_a(); }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MultipleMarkers));
        assert!(!graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::DuplicateId));
        let node = test_node(&graph, "tests/AuthTests:AuthTests:T");
        assert!(node.content.contains("single_a();"));
        assert!(!node.content.contains("multi();"));
    }

    // [FR-057]
    #[test]
    fn マーカーのない同名出現はまとめられない() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { marked_body(); }\n\n    [Fact]\n    public void T(int x) { unmarked_body(); }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let node = test_node(&graph, "tests/AuthTests:AuthTests:T");
        assert!(node.content.contains("marked_body();"));
        assert!(!node.content.contains("unmarked_body();"));
    }

    // [FR-014]
    #[test]
    fn 構文エラーのcsharpテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[(
            "tests/Broken.cs",
            "public class A {\n    [Fact]\n    public void T( {\n}\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("Broken.cs")));
    }

    // [FR-059]
    #[test]
    fn javaファイルからtestノードとエッジが作られる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.java",
                "public class AuthTests {\n    // [FR-001]\n    @Test\n    void t() { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(has_test_node(&graph, "tests/AuthTests:AuthTests:t"));
        assert!(graph
            .edges
            .contains_key("test-req:tests/AuthTests:AuthTests:t:FR-001"));
    }

    // [FR-059]
    #[test]
    fn kotlinファイルからtestノードとエッジが作られる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTest.kt",
                "class AuthTest {\n    // [FR-001]\n    @Test\n    fun `一覧 取得`() { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(has_test_node(&graph, "tests/AuthTest:AuthTest:一覧 取得"));
        assert!(graph
            .edges
            .contains_key("test-req:tests/AuthTest:AuthTest:一覧 取得:FR-001"));
    }

    // [FR-059]
    #[test]
    fn kotlinスクリプトのktsファイルは対象外になる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/build.kts",
                "class AuthTest {\n    // [FR-001]\n    @Test\n    fun t() { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
    }

    // [FR-057]
    #[test]
    fn javaのオーバーロードは1つのtestにまとめられる() {
        let src = "public class AuthTests {\n    // [FR-001]\n    @Test\n    void t() { one(); }\n\n    // [FR-001]\n    @Test\n    void t(int x) { two(); }\n}\n";
        let source = FakeSource::new(&[REQ1, ("tests/AuthTests.java", src)]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let tests: Vec<_> = graph
            .nodes
            .keys()
            .filter(|k| k.kind == NodeKind::Test)
            .collect();
        assert_eq!(tests.len(), 1, "{tests:?}");
        let edges: Vec<_> = graph
            .edges
            .values()
            .filter(|e| e.kind == crate::model::EdgeKind::TestReq)
            .collect();
        assert_eq!(edges.len(), 1, "{edges:?}");
        let node = test_node(&graph, "tests/AuthTests:AuthTests:t");
        assert!(node.content.contains("one();"));
        assert!(node.content.contains("two();"));
    }

    // [FR-014]
    #[test]
    fn 構文エラーのkotlinテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[("tests/Broken.kt", "class A { fun x() {} }\n")]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("Broken.kt")));
    }

    // [FR-014]
    #[test]
    fn 構文エラーのjavaテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[(
            "tests/Broken.java",
            "public class A {\n    @Test\n    void t( {\n}\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("Broken.java")));
    }

    // [FR-014]
    #[test]
    fn 構文エラーのtypescriptテストファイルはparse_errorとしてinvalidになる() {
        let source = FakeSource::new(&[("src/broken.test.ts", "it(\"a\", () => {\n")]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::ParseError && i.target.contains("broken.test.ts")));
    }

    // [FR-014]
    #[test]
    fn csharp_1つのテストに2件以上のマーカーがあるとinvalidになる() {
        let source = FakeSource::new(&[(
            "tests/AuthTests.cs",
            "public class AuthTests\n{\n    // [FR-001]\n    // [FR-002]\n    [Fact]\n    public void T() { }\n}\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MultipleMarkers));
    }

    // [FR-014]
    #[test]
    fn typescript_1つのテストに2件以上のマーカーがあるとinvalidになる() {
        let source = FakeSource::new(&[(
            "src/a.test.ts",
            "// [FR-001]\n// [FR-002]\nit(\"a\", () => {});\n",
        )]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MultipleMarkers));
    }

    // [FR-018]
    #[test]
    fn testsのディレクトリ指定はcsharpとtypescriptのファイルにも適用される() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "other/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { }\n}\n",
            ),
            ("other/a.test.ts", "// [FR-001]\nit(\"a\", () => {});\n"),
        ]);
        let mut cfg = config();
        cfg.tests = vec!["tests".to_string()];
        let graph = build_graph(&source, &cfg).unwrap();
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
    }

    fn exclude_config(names: &[&str]) -> Config {
        let mut cfg = config();
        cfg.exclude = names.iter().map(|s| s.to_string()).collect();
        cfg
    }

    const MARKED_JS: &str = "// [FR-001]\nit(\"works\", () => {});\n";

    // [FR-065]
    #[test]
    fn excludeに指定した名前のディレクトリ配下のtestはノードにならない() {
        let source = FakeSource::new(&[
            REQ1,
            ("build/a.test.js", MARKED_JS),
            ("src/a.test.js", MARKED_JS),
        ]);
        let graph = build_graph(&source, &exclude_config(&["build"])).unwrap();
        assert!(!has_test_node(&graph, "build/a.test::works"));
        assert!(has_test_node(&graph, "src/a.test::works"));
    }

    // [FR-065]
    #[test]
    fn excludeが空なら全ディレクトリのtestがノードになる() {
        let source = FakeSource::new(&[
            REQ1,
            ("build/a.test.js", MARKED_JS),
            ("src/a.test.js", MARKED_JS),
        ]);
        let graph = build_graph(&source, &exclude_config(&[])).unwrap();
        assert!(has_test_node(&graph, "build/a.test::works"));
        assert!(has_test_node(&graph, "src/a.test::works"));
    }

    // [FR-065]
    #[test]
    fn excludeはネストした深さのディレクトリにも適用される() {
        let source = FakeSource::new(&[
            REQ1,
            ("pkg/build/b.test.js", MARKED_JS),
            ("pkg/src/b.test.js", MARKED_JS),
        ]);
        let graph = build_graph(&source, &exclude_config(&["build"])).unwrap();
        assert!(!has_test_node(&graph, "pkg/build/b.test::works"));
        assert!(has_test_node(&graph, "pkg/src/b.test::works"));
    }

    // [FR-065]
    #[test]
    fn excludeに既定の除外名と重複する名前を指定しても問題ない() {
        let source = FakeSource::new(&[
            REQ1,
            ("dist/a.test.js", MARKED_JS),
            ("src/a.test.js", MARKED_JS),
        ]);
        let graph = build_graph(&source, &exclude_config(&["dist"])).unwrap();
        assert!(!has_test_node(&graph, "dist/a.test::works"));
        assert!(has_test_node(&graph, "src/a.test::works"));
    }

    // [FR-065]
    #[test]
    fn excludeは大文字小文字を区別する() {
        let source = FakeSource::new(&[REQ1, ("build/a.test.js", MARKED_JS)]);
        let graph = build_graph(&source, &exclude_config(&["Build"])).unwrap();
        assert!(has_test_node(&graph, "build/a.test::works"));
    }

    // [FR-065]
    #[test]
    fn exclude_nextのようなドット始まりの名前も除外できる() {
        let source = FakeSource::new(&[
            REQ1,
            (".next/a.test.js", MARKED_JS),
            ("src/a.test.js", MARKED_JS),
        ]);
        let graph = build_graph(&source, &exclude_config(&[".next"])).unwrap();
        assert!(!has_test_node(&graph, ".next/a.test::works"));
        assert!(has_test_node(&graph, "src/a.test::works"));
    }

    // [FR-067]
    #[test]
    fn 予約名のindex_mdとlog_mdはdocsでもrequirementsでもノードにならない() {
        let source = FakeSource::new(&[
            (
                "docs/index.md",
                "---\ntype: BusinessNeed\nid: DOC-X\n---\n[r](../req/fr1.md)\n",
            ),
            ("docs/log.md", "---\ntype: BusinessNeed\nid: DOC-L\n---\n"),
            ("docs/sub/index.md", "---\ntype: BusinessNeed\n---\n"),
            ("req/index.md", "---\ntype: Requirement\nid: FR-001\n---\n"),
            ("req/log.md", "---\ntype: Requirement\n---\n"),
            (
                "req/fr1.md",
                "---\ntype: Requirement\nid: FR-001\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(
            graph.invalid.is_empty(),
            "reserved names must not be invalid: {:?}",
            graph.invalid
        );
        assert!(!graph.nodes.values().any(|n| n.key.kind == NodeKind::Doc));
        let reqs: Vec<&str> = graph
            .nodes
            .values()
            .filter(|n| n.key.kind == NodeKind::Req)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(reqs, ["req/fr1.md"]);
        assert!(graph.edges.is_empty(), "{:?}", graph.edges);
    }

    // [FR-066]
    #[test]
    fn index未設定なら索引のfindingを一切出さない() {
        let source = FakeSource::new(&[
            ("docs/a.md", "---\ntype: BusinessNeed\nid: DOC-A\n---\n"),
            ("docs/untyped.md", "plain\n"),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.index_issues.is_empty(), "{:?}", graph.index_issues);
    }

    // [FR-067]
    #[test]
    fn index設定があっても索引のtype_business_needはノードにならない() {
        let source = FakeSource::new(&[
            (
                "docs/index.md",
                "---\ntype: BusinessNeed\nid: DOC-X\n---\n- [A](a.md) — D\n",
            ),
            (
                "docs/a.md",
                "---\ntype: BusinessNeed\nid: DOC-A\ntitle: A\ndescription: D\n---\n",
            ),
        ]);
        let mut cfg = config();
        cfg.index = vec!["docs".to_string()];
        let graph = build_graph(&source, &cfg).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        let docs: Vec<&str> = graph
            .nodes
            .values()
            .filter(|n| n.key.kind == NodeKind::Doc)
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(docs, ["docs/a.md"]);
        assert!(graph.index_issues.is_empty(), "{:?}", graph.index_issues);
    }

    // [FR-073]
    #[test]
    fn index設定があればbuild_graphが索引のfindingを集める() {
        let source = FakeSource::new(&[
            (
                "docs/a.md",
                "---\ntype: BusinessNeed\nid: DOC-A\ntitle: A\ndescription: D\n---\n",
            ),
            ("docs/untyped.md", "plain\n"),
        ]);
        let mut cfg = config();
        cfg.index = vec!["docs".to_string()];
        let graph = build_graph(&source, &cfg).unwrap();
        assert!(graph
            .index_issues
            .iter()
            .any(|i| i.reason == ReasonCode::MissingIndex && i.target == "docs"));
        assert!(graph
            .index_issues
            .iter()
            .any(|i| i.reason == ReasonCode::UntypedFile && i.target == "docs/untyped.md"));
    }

    fn indexed_config(roots: &[&str]) -> Config {
        let mut cfg = config();
        cfg.index = roots.iter().map(|r| r.to_string()).collect();
        cfg
    }

    fn desc_edges(graph: &crate::model::Graph) -> Vec<&crate::model::Edge> {
        graph
            .edges
            .values()
            .filter(|e| e.kind == crate::model::EdgeKind::Desc)
            .collect()
    }

    const DOC_A: &str = "---\ntype: BusinessNeed\nid: DOC-A\ndescription: 要約A\n---\n本文A\n";
    const REQ_1: &str = "---\ntype: Requirement\nid: FR-001\ndescription: 要約1\n---\n本文1\n";

    // [FR-086]
    #[test]
    fn index配下のdocとreqにdescエッジが1本ずつできる() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A), ("docs/r.md", REQ_1)]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        let doc = graph.edges.get("desc:doc:DOC-A").expect("desc:doc:DOC-A");
        assert_eq!(doc.kind, crate::model::EdgeKind::Desc);
        assert_eq!(doc.from, doc.to);
        assert_eq!(doc.from.kind, NodeKind::Doc);
        assert_eq!(doc.from.id, "DOC-A");
        let req = graph.edges.get("desc:req:FR-001").expect("desc:req:FR-001");
        assert_eq!(req.kind, crate::model::EdgeKind::Desc);
        assert_eq!(req.from, req.to);
        assert_eq!(req.from.kind, NodeKind::Req);
        assert_eq!(desc_edges(&graph).len(), 2, "{:?}", graph.edges);
    }

    // [FR-086]
    #[test]
    fn descエッジはdescriptionが無いノードにもできる() {
        let source = FakeSource::new(&[(
            "docs/a.md",
            "---\ntype: BusinessNeed\nid: DOC-A\n---\n本文A\n",
        )]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert!(
            graph.edges.contains_key("desc:doc:DOC-A"),
            "{:?}",
            graph.edges
        );
    }

    // [FR-086]
    #[test]
    fn descエッジは空や改行を含むdescriptionのノードにもできる() {
        let source = FakeSource::new(&[
            (
                "docs/a.md",
                "---\ntype: BusinessNeed\nid: DOC-A\ndescription: \"\"\n---\n本文\n",
            ),
            (
                "docs/b.md",
                "---\ntype: BusinessNeed\nid: DOC-B\ndescription: |\n  一行目\n  二行目\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert!(
            graph.edges.contains_key("desc:doc:DOC-A"),
            "{:?}",
            graph.edges
        );
        assert!(
            graph.edges.contains_key("desc:doc:DOC-B"),
            "{:?}",
            graph.edges
        );
    }

    // [FR-086]
    #[test]
    fn indexが空ならdescエッジは1本もできない() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A), ("docs/r.md", REQ_1)]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(desc_edges(&graph).is_empty(), "{:?}", graph.edges);
    }

    // [FR-086]
    #[test]
    fn 索引の根の外にあるreqにはdescエッジができない() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A), ("req/fr1.md", REQ_1)]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert!(graph.edges.contains_key("desc:doc:DOC-A"));
        assert!(
            !graph.edges.contains_key("desc:req:FR-001"),
            "{:?}",
            graph.edges
        );
    }

    // [FR-086]
    #[test]
    fn 索引の根の配下にあるreqにはdescエッジができる() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A), ("req/fr1.md", REQ_1)]);
        let graph = build_graph(&source, &indexed_config(&["docs", "req"])).unwrap();
        assert!(
            graph.edges.contains_key("desc:req:FR-001"),
            "{:?}",
            graph.edges
        );
    }

    // [FR-086]
    #[test]
    fn 無効なidのノードにはdescエッジができない() {
        let source = FakeSource::new(&[
            (
                "docs/bad.md",
                "---\ntype: BusinessNeed\nid: BAD ID\ndescription: D\n---\n本文\n",
            ),
            (
                "docs/noid.md",
                "---\ntype: BusinessNeed\ndescription: D\n---\n本文\n",
            ),
        ]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert!(!graph.invalid.is_empty());
        assert!(desc_edges(&graph).is_empty(), "{:?}", graph.edges);
    }

    // [FR-086]
    #[test]
    fn guideなどmiwaのノードでない文書にはdescエッジができない() {
        let source = FakeSource::new(&[(
            "docs/g.md",
            "---\ntype: Guide\nid: GUIDE-1\ndescription: D\n---\n本文\n",
        )]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert!(desc_edges(&graph).is_empty(), "{:?}", graph.edges);
    }

    // [FR-086]
    #[test]
    fn descエッジはorphanとno_testの判定に影響しない() {
        let source = FakeSource::new(&[("docs/r.md", REQ_1), ("docs/a.md", DOC_A)]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        let findings = crate::check::check(&graph, &crate::lock::Lock::default());
        assert!(findings
            .iter()
            .any(|f| f.kind == crate::check::FindingKind::Orphan
                && f.reason == ReasonCode::ReqWithoutDoc
                && f.target == "FR-001"));
        assert!(findings
            .iter()
            .any(|f| f.kind == crate::check::FindingKind::NoTest && f.target == "FR-001"));
        assert!(findings
            .iter()
            .any(|f| f.kind == crate::check::FindingKind::Orphan
                && f.reason == ReasonCode::DocWithoutReq
                && f.target == "DOC-A"));
    }

    // [FR-086]
    #[test]
    fn ロックにdescエッジがありノードが無ければremovedになる() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A)]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        let mut lock = crate::lock::Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-GONE".to_string(),
            crate::lock::LockEntry {
                from: "sha256:x".to_string(),
                to: "sha256:y".to_string(),
                by: "human".to_string(),
                reason: None,
                score: None,
                at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        let findings = crate::check::check(&graph, &lock);
        assert!(findings
            .iter()
            .any(|f| f.kind == crate::check::FindingKind::Removed
                && f.target == "desc:doc:DOC-GONE"
                && f.reason == ReasonCode::RemovedEdge));
    }

    // [FR-086]
    #[test]
    fn indexの根から外れるとロックのdescエッジはremovedになる() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A)]);
        let with_index = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        let mut lock = crate::lock::Lock::default();
        let (from, to) =
            crate::model::edge_hashes(&with_index, &with_index.edges["desc:doc:DOC-A"]);
        lock.edges.insert(
            "desc:doc:DOC-A".to_string(),
            crate::lock::LockEntry {
                from,
                to,
                by: "human".to_string(),
                reason: None,
                score: None,
                at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        let without_index = build_graph(&source, &config()).unwrap();
        let findings = crate::check::check(&without_index, &lock);
        assert!(findings
            .iter()
            .any(|f| f.kind == crate::check::FindingKind::Removed && f.target == "desc:doc:DOC-A"));
    }

    fn desc_hashes(graph: &crate::model::Graph, id: &str) -> (String, String) {
        crate::model::edge_hashes(graph, &graph.edges[id])
    }

    // [FR-087]
    #[test]
    fn descエッジのfromは要約_toは本文のハッシュになる() {
        let source = FakeSource::new(&[("docs/a.md", DOC_A)]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert_eq!(
            desc_hashes(&graph, "desc:doc:DOC-A"),
            (
                crate::hash::content_hash("要約A"),
                crate::hash::content_hash("本文A\n")
            )
        );
    }

    // [FR-087]
    #[test]
    fn descエッジのfromは前後の空白を除いた要約のハッシュになる() {
        let source = FakeSource::new(&[(
            "docs/a.md",
            "---\ntype: BusinessNeed\nid: DOC-A\ndescription: \"  要約A  \"\n---\n本文A\n",
        )]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert_eq!(
            desc_hashes(&graph, "desc:doc:DOC-A").0,
            crate::hash::content_hash("要約A")
        );
    }

    // [FR-087]
    #[test]
    fn descエッジのfromは要約が無い_空_改行を含むときは空文字列のハッシュになる() {
        let empty = crate::hash::content_hash("");
        for fm in [
            "type: BusinessNeed\nid: DOC-A",
            "type: BusinessNeed\nid: DOC-A\ndescription: \"\"",
            "type: BusinessNeed\nid: DOC-A\ndescription: |\n  一行目\n  二行目",
        ] {
            let source = FakeSource::new(&[("docs/a.md", &format!("---\n{fm}\n---\n本文\n"))]);
            let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
            assert_eq!(desc_hashes(&graph, "desc:doc:DOC-A").0, empty, "{fm}");
        }
    }

    // [FR-087]
    #[test]
    fn descエッジのfromは折り畳みスカラーの要約を1行に畳んだハッシュになる() {
        let source = FakeSource::new(&[(
            "docs/a.md",
            "---\ntype: BusinessNeed\nid: DOC-A\ndescription: >\n  一行目\n  二行目\n---\n本文\n",
        )]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert_eq!(
            desc_hashes(&graph, "desc:doc:DOC-A").0,
            crate::hash::content_hash("一行目 二行目")
        );
    }

    // [FR-086]
    #[test]
    fn 同じidのdocとreqは別々のdescエッジになる() {
        let source = FakeSource::new(&[
            (
                "docs/a.md",
                "---\ntype: BusinessNeed\nid: X-1\ndescription: 要約D\n---\n本文D\n",
            ),
            (
                "docs/r.md",
                "---\ntype: Requirement\nid: X-1\ndescription: 要約R\n---\n本文R\n",
            ),
        ]);
        let graph = build_graph(&source, &indexed_config(&["docs"])).unwrap();
        assert!(
            graph.edges.contains_key("desc:doc:X-1"),
            "{:?}",
            graph.edges
        );
        assert!(
            graph.edges.contains_key("desc:req:X-1"),
            "{:?}",
            graph.edges
        );
        assert_eq!(desc_edges(&graph).len(), 2, "{:?}", graph.edges);
        assert_eq!(
            desc_hashes(&graph, "desc:doc:X-1").0,
            crate::hash::content_hash("要約D")
        );
        assert_eq!(
            desc_hashes(&graph, "desc:req:X-1").0,
            crate::hash::content_hash("要約R")
        );
    }

    // [FR-087]
    #[test]
    fn 要約だけを変えてもdoc_reqとtest_reqのハッシュは変わらずfindingも出ない() {
        let build = |description: &str| {
            let source = FakeSource::new(&[
                (
                    "docs/a.md",
                    &format!(
                        "---\ntype: BusinessNeed\nid: DOC-A\ndescription: {description}\n---\n[r](../req/fr1.md)\n"
                    ),
                ),
                (
                    "req/fr1.md",
                    "---\ntype: Requirement\nid: FR-001\n---\n本文1\n",
                ),
                ("tests/t.rs", "// [FR-001]\n#[test]\nfn t() {}\n"),
            ]);
            build_graph(&source, &indexed_config(&["docs"])).unwrap()
        };
        let old = build("旧要約");
        let new = build("新要約");
        let test_edge = old
            .edges
            .keys()
            .find(|k| k.starts_with("test-req:"))
            .cloned()
            .expect("test-req edge");
        for id in ["doc-req:DOC-A:FR-001", test_edge.as_str()] {
            assert_eq!(desc_hashes(&old, id), desc_hashes(&new, id), "{id}");
        }
        assert_ne!(
            desc_hashes(&old, "desc:doc:DOC-A").0,
            desc_hashes(&new, "desc:doc:DOC-A").0
        );
        let mut lock = crate::lock::Lock::default();
        for (id, edge) in &old.edges {
            let (from, to) = crate::model::edge_hashes(&old, edge);
            lock.edges.insert(
                id.clone(),
                crate::lock::LockEntry {
                    from,
                    to,
                    by: "human".to_string(),
                    reason: None,
                    score: None,
                    at: "2026-01-01T00:00:00Z".to_string(),
                },
            );
        }
        let drifts: Vec<String> = crate::check::check(&new, &lock)
            .into_iter()
            .filter(|f| f.kind == crate::check::FindingKind::Drift)
            .map(|f| f.target)
            .collect();
        assert_eq!(drifts, vec!["desc:doc:DOC-A".to_string()]);
    }
}
