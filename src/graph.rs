//! Build the doc/req/test dependency graph from a `Source`.

pub fn build_graph(
    source: &dyn crate::source::Source,
    config: &crate::config::Config,
) -> anyhow::Result<crate::model::Graph> {
    use crate::model::{
        edge_id, is_valid_id, Broken, Edge, EdgeKind, Invalid, Node, NodeKey, NodeKind, ReasonCode,
        TargetKind,
    };
    use std::collections::{BTreeMap, BTreeSet};

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

    type Extractor =
        fn(&str, &str) -> Result<Vec<crate::extract::ExtractedTest>, crate::extract::ExtractError>;
    let mut extracted_tests: Vec<(String, crate::extract::ExtractedTest)> = Vec::new();
    for path in &files {
        if !is_under(path, &config.tests) {
            continue;
        }
        let extractor: Extractor = if path.ends_with("_test.go") {
            crate::extract::extract_go_tests
        } else if path.ends_with(".rs") {
            crate::extract::extract_rust_tests
        } else if path.ends_with(".cs") {
            crate::extract::extract_csharp_tests
        } else if crate::extract::is_typescript_test_file(path) {
            crate::extract::extract_typescript_tests
        } else {
            continue;
        };
        let bytes = source.read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        match extractor(path, &text) {
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
}
