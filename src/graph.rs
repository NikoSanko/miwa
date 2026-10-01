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
        } else if [".test.ts", ".spec.ts", ".test.tsx", ".spec.tsx"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
        {
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

    let mut marked_paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, t) in &extracted_tests {
        marked_paths
            .entry(t.id.clone())
            .or_default()
            .push(path.clone());
    }
    for (id, paths) in &marked_paths {
        if paths.len() > 1 {
            let mut unique_paths = paths.clone();
            unique_paths.dedup();
            graph.invalid.push(Invalid {
                target_kind: TargetKind::Test,
                target: id.clone(),
                reason: ReasonCode::DuplicateId,
                detail: format!("duplicate test id {id} at {}", unique_paths.join(", ")),
            });
        }
    }

    for (path, t) in extracted_tests {
        if marked_paths.get(&t.id).is_some_and(|p| p.len() > 1) {
            continue;
        }
        if t.markers.len() > 1 {
            graph.invalid.push(Invalid {
                target_kind: TargetKind::Test,
                target: t.id.clone(),
                reason: ReasonCode::MultipleMarkers,
                detail: format!("{} has multiple markers: {:?}", t.id, t.markers),
            });
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
                content: t.content,
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

    // [FR-057]
    #[test]
    fn csharpのオーバーロードは識別子ごとに1件のduplicate_idとなりノードもエッジも作られない() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { }\n}\n",
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
        assert_eq!(dups[0].target, "tests/AuthTests:AuthTests:T");
        assert!(dups[0].detail.contains("tests/AuthTests.cs"));
        assert_eq!(dups[0].detail.matches("tests/AuthTests.cs").count(), 1);
        assert!(!graph.nodes.keys().any(|k| k.kind == NodeKind::Test));
        assert!(graph
            .edges
            .values()
            .all(|e| e.kind != crate::model::EdgeKind::TestReq));
    }

    // [FR-057]
    #[test]
    fn 重複したtestだけが検証だったreqにはno_testが出る() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        let findings = crate::check::check(&graph, &crate::lock::Lock::default());
        assert!(findings
            .iter()
            .any(|f| f.kind == crate::check::FindingKind::NoTest && f.target == "FR-001"));
    }

    // [FR-057]
    #[test]
    fn goの別ファイルにある同名testはduplicate_idになる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "pkg/a_test.go",
                "package pkg\n\n// [FR-001]\nfunc TestX(t *testing.T) {}\n",
            ),
            (
                "pkg/b_test.go",
                "package pkg\n\n// [FR-001]\nfunc TestX(t *testing.T) {}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        let dups: Vec<_> = graph
            .invalid
            .iter()
            .filter(|i| i.reason == ReasonCode::DuplicateId)
            .collect();
        assert_eq!(dups.len(), 1, "{:?}", graph.invalid);
        assert_eq!(dups[0].target, "pkg:TestX");
        assert!(dups[0].detail.contains("pkg/a_test.go"));
        assert!(dups[0].detail.contains("pkg/b_test.go"));
        assert!(!has_test_node(&graph, "pkg:TestX"));
    }

    // [FR-057]
    #[test]
    fn typescriptの同名testはduplicate_idになる() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "src/a.test.ts",
                "// [FR-001]\nit(\"same\", () => {});\n\n// [FR-001]\nit(`same`, () => {});\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::DuplicateId && i.target == "src/a.test::same"));
        assert!(!has_test_node(&graph, "src/a.test::same"));
    }

    // [FR-057]
    #[test]
    fn 複数マーカーかつ重複のtestはduplicate_idのみ報告されmultiple_markersは出ない() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    // [FR-001]\n    // [FR-002]\n    [Fact]\n    public void T() { }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::DuplicateId));
        assert!(!graph
            .invalid
            .iter()
            .any(|i| i.reason == ReasonCode::MultipleMarkers));
    }

    // [FR-057]
    #[test]
    fn マーカーのないtestの同名は重複として扱われない() {
        let source = FakeSource::new(&[
            REQ1,
            (
                "tests/AuthTests.cs",
                "public class AuthTests\n{\n    [Fact]\n    public void T() { }\n\n    [Fact]\n    public void T(int x) { }\n\n    // [FR-001]\n    [Fact]\n    public void U() { }\n}\n",
            ),
        ]);
        let graph = build_graph(&source, &config()).unwrap();
        assert!(graph.invalid.is_empty(), "{:?}", graph.invalid);
        assert!(has_test_node(&graph, "tests/AuthTests:AuthTests:U"));
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
