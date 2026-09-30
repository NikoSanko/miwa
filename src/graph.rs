//! Build the doc/req/test dependency graph from a `Source`.

pub fn build_graph(
    source: &dyn crate::source::Source,
    config: &crate::config::Config,
) -> anyhow::Result<crate::model::Graph> {
    let _ = (source, config);
    todo!()
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
                llm_model: "x".to_string(),
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
}
