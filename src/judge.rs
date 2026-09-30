//! `judge`: semantic judgment of drifted / new edges (Jev first, then an LLM agent).

use crate::model::EdgeKind;

pub const MAX_BODY_BYTES: usize = 200_000;
pub const JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const JEV_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
/// テスト専用: 設定されていれば Jev の endpoint を上書きする
pub const JEV_ENDPOINT_OVERRIDE_ENV: &str = "MIWA_JEV_ENDPOINT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Consistent,
    Review,
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeInput {
    pub edge_id: String,
    pub kind: EdgeKind,
    pub from_path: String,
    pub req_path: String,
    pub from_text: String,
    pub req_text: String,
    pub from_hash: String,
    pub to_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Judgment {
    pub edge: String,
    pub kind: EdgeKind,
    pub verdict: Verdict,
    pub score: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum JudgeError {
    #[error("environment variable {0} is not set")]
    MissingApiKey(String),
    #[error("edge {0} is not a judge target")]
    NotATarget(String),
    #[error("api.typesafe.ai returned HTTP {status}")]
    HttpStatus { status: u16 },
    #[error("api.typesafe.ai request failed: {message}")]
    Transport { message: String },
    #[error("invalid Jev response: {0}")]
    InvalidJevResponse(String),
    #[error("failed to start agent command: {0}")]
    AgentSpawn(String),
    #[error("agent command exited with status {0}")]
    AgentFailed(i32),
}

/// drift / new のエッジを対象に EdgeInput を組む（エッジ ID 昇順）。requested が非空ならその ID だけ（重複は 1 つに）。対象外の ID は NotATarget
pub fn select_targets(
    graph: &crate::model::Graph,
    lock: &crate::lock::Lock,
    findings: &[crate::check::Finding],
    requested: &[String],
) -> Result<Vec<EdgeInput>, JudgeError> {
    let _ = (graph, lock, findings, requested);
    todo!()
}

pub fn build_jev_request(input: &EdgeInput) -> serde_json::Value {
    let _ = input;
    todo!()
}

pub fn parse_jev_response(body: &str) -> Result<f64, JudgeError> {
    let _ = body;
    todo!()
}

pub type HttpCall<'a> = &'a dyn Fn(&serde_json::Value) -> Result<String, JudgeError>;

/// 各エッジを一次判定。本文合計が MAX_BODY_BYTES 超なら Jev を呼ばず Skipped（score None）。score >= threshold で Consistent、未満で Review。順序はエッジ ID 昇順。Jev のエラーはそのまま Err
pub fn judge_edges(
    inputs: &[EdgeInput],
    threshold: f64,
    jev: HttpCall,
) -> Result<Vec<Judgment>, JudgeError> {
    let _ = (inputs, threshold, jev);
    todo!()
}

/// Review のエッジだけを含むプロンプト（FR-046 / FR-047）。Review が 0 件なら空文字列
pub fn build_agent_prompt(
    inputs: &[EdgeInput],
    judgments: &[Judgment],
    auto_reconcile: bool,
) -> String {
    let _ = (inputs, judgments, auto_reconcile);
    todo!()
}

/// command[0] を起動し、prompt を stdin で渡す。stdout / stderr は継承。非 0 終了は AgentFailed、起動失敗は AgentSpawn
pub fn run_agent(
    command: &[String],
    prompt: &str,
    cwd: &std::path::Path,
) -> Result<(), JudgeError> {
    let _ = (command, prompt, cwd);
    todo!()
}

/// Consistent のエッジだけを by: "jev" + score で記録した新しい Lock を返す（元の Lock は変更しない）
pub fn apply_auto_reconcile(
    lock: &crate::lock::Lock,
    inputs: &[EdgeInput],
    judgments: &[Judgment],
    now: &str,
) -> crate::lock::Lock {
    let _ = (lock, inputs, judgments, now);
    todo!()
}

/// 1 行 1 エッジ `<consistent|review|skipped> <edge id> score=<0.00〜1.00 | ->`。0 件なら "対象なし\n"
pub fn render_text(judgments: &[Judgment]) -> String {
    let _ = judgments;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::check;
    use crate::lock::{Lock, LockEntry};
    use crate::model::{
        Broken, Edge, Graph, Invalid, Node, NodeKey, NodeKind, ReasonCode, TargetKind,
    };
    use std::cell::Cell;

    fn add_node(
        graph: &mut Graph,
        kind: NodeKind,
        id: &str,
        path: &str,
        content: &str,
        hash: &str,
    ) {
        let key = NodeKey {
            kind,
            id: id.to_string(),
        };
        graph.nodes.insert(
            key.clone(),
            Node {
                key,
                path: path.to_string(),
                content: content.to_string(),
                hash: hash.to_string(),
            },
        );
    }

    fn add_edge(graph: &mut Graph, kind: EdgeKind, from: &str, to: &str) -> String {
        let id = crate::model::edge_id(kind, from, to);
        let from_kind = match kind {
            EdgeKind::DocReq => NodeKind::Doc,
            EdgeKind::TestReq => NodeKind::Test,
        };
        graph.edges.insert(
            id.clone(),
            Edge {
                id: id.clone(),
                kind,
                from: NodeKey {
                    kind: from_kind,
                    id: from.to_string(),
                },
                to: NodeKey {
                    kind: NodeKind::Req,
                    id: to.to_string(),
                },
            },
        );
        id
    }

    fn lock_entry(from: &str, to: &str) -> LockEntry {
        LockEntry {
            from: from.to_string(),
            to: to.to_string(),
            by: "human".to_string(),
            reason: Some("既存の合意".to_string()),
            score: None,
            at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    /// new / drift / 変化なし / test-req new / broken 関連 / invalid 関連 / removed が混在するグラフ
    fn mixed_fixture() -> (Graph, Lock) {
        let mut graph = Graph::default();
        // new: doc-req:DOC-A:FR-001
        add_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-A",
            "docs/a.md",
            "doc a",
            "sha256:da",
        );
        add_node(
            &mut graph,
            NodeKind::Req,
            "FR-001",
            "req/fr1.md",
            "req 1",
            "sha256:r1",
        );
        add_edge(&mut graph, EdgeKind::DocReq, "DOC-A", "FR-001");
        // drift: doc-req:DOC-B:FR-002
        add_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-B",
            "docs/b.md",
            "doc b",
            "sha256:db-new",
        );
        add_node(
            &mut graph,
            NodeKind::Req,
            "FR-002",
            "req/fr2.md",
            "req 2",
            "sha256:r2",
        );
        add_edge(&mut graph, EdgeKind::DocReq, "DOC-B", "FR-002");
        // 変化なし: doc-req:DOC-C:FR-003
        add_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-C",
            "docs/c.md",
            "doc c",
            "sha256:dc",
        );
        add_node(
            &mut graph,
            NodeKind::Req,
            "FR-003",
            "req/fr3.md",
            "req 3",
            "sha256:r3",
        );
        add_edge(&mut graph, EdgeKind::DocReq, "DOC-C", "FR-003");
        // test-req new
        add_node(
            &mut graph,
            NodeKind::Test,
            "tests/t::checks",
            "tests/t.rs",
            "fn checks() {}",
            "sha256:t1",
        );
        add_edge(&mut graph, EdgeKind::TestReq, "tests/t::checks", "FR-001");
        // broken 関連: DOC-E から壊れたリンクがあり、かつ new のエッジも持つ
        add_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-E",
            "docs/e.md",
            "doc e",
            "sha256:de",
        );
        add_node(
            &mut graph,
            NodeKind::Req,
            "FR-005",
            "req/fr5.md",
            "req 5",
            "sha256:r5",
        );
        add_edge(&mut graph, EdgeKind::DocReq, "DOC-E", "FR-005");
        graph.broken.push(Broken {
            from: NodeKey {
                kind: NodeKind::Doc,
                id: "DOC-E".to_string(),
            },
            target: "../req/missing.md".to_string(),
            reason: ReasonCode::MissingReq,
            detail: "missing".to_string(),
        });
        // invalid 関連: FR-004 が重複 id で invalid、かつ new のエッジを持つ
        add_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-D",
            "docs/d.md",
            "doc d",
            "sha256:dd",
        );
        add_node(
            &mut graph,
            NodeKind::Req,
            "FR-004",
            "req/fr4.md",
            "req 4",
            "sha256:r4",
        );
        add_edge(&mut graph, EdgeKind::DocReq, "DOC-D", "FR-004");
        graph.invalid.push(Invalid {
            target_kind: TargetKind::Req,
            target: "FR-004".to_string(),
            reason: ReasonCode::DuplicateId,
            detail: "dup".to_string(),
        });

        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-B:FR-002".to_string(),
            lock_entry("sha256:db-old", "sha256:r2"),
        );
        lock.edges.insert(
            "doc-req:DOC-C:FR-003".to_string(),
            lock_entry("sha256:dc", "sha256:r3"),
        );
        // removed: ソースに存在しない
        lock.edges.insert(
            "doc-req:DOC-GONE:FR-009".to_string(),
            lock_entry("sha256:x", "sha256:y"),
        );
        (graph, lock)
    }

    fn select(requested: &[&str]) -> Result<Vec<EdgeInput>, JudgeError> {
        let (graph, lock) = mixed_fixture();
        let findings = check(&graph, &lock);
        let requested: Vec<String> = requested.iter().map(|s| s.to_string()).collect();
        select_targets(&graph, &lock, &findings, &requested)
    }

    fn input(id: &str, kind: EdgeKind, from_text: &str, req_text: &str) -> EdgeInput {
        EdgeInput {
            edge_id: id.to_string(),
            kind,
            from_path: "docs/from.md".to_string(),
            req_path: "req/req.md".to_string(),
            from_text: from_text.to_string(),
            req_text: req_text.to_string(),
            from_hash: "sha256:from".to_string(),
            to_hash: "sha256:to".to_string(),
        }
    }

    fn jev_body(score: f64) -> String {
        format!(
            r#"{{"model":"jev-1.13.0","answers":{{"consistent":{{"type":"noul","noul":{score}}}}},"usage":{{"input_tokens":1,"output_tokens":1}}}}"#
        )
    }

    fn judgment(edge: &str, verdict: Verdict, score: Option<f64>) -> Judgment {
        Judgment {
            edge: edge.to_string(),
            kind: EdgeKind::DocReq,
            verdict,
            score,
        }
    }

    // [FR-038]
    #[test]
    fn select_targetsはdriftとnewのエッジだけをエッジid昇順で返す() {
        let targets = select(&[]).unwrap();
        let ids: Vec<&str> = targets.iter().map(|t| t.edge_id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "doc-req:DOC-A:FR-001",
                "doc-req:DOC-B:FR-002",
                "test-req:tests/t::checks:FR-001",
            ]
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsはエッジの両端のパス_本文_ハッシュをedge_inputに詰める() {
        let targets = select(&["doc-req:DOC-B:FR-002"]).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0],
            EdgeInput {
                edge_id: "doc-req:DOC-B:FR-002".to_string(),
                kind: EdgeKind::DocReq,
                from_path: "docs/b.md".to_string(),
                req_path: "req/fr2.md".to_string(),
                from_text: "doc b".to_string(),
                req_text: "req 2".to_string(),
                from_hash: "sha256:db-new".to_string(),
                to_hash: "sha256:r2".to_string(),
            }
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsはbrokenやinvalidに関わるエッジとremovedを対象にしない() {
        let targets = select(&[]).unwrap();
        let ids: Vec<&str> = targets.iter().map(|t| t.edge_id.as_str()).collect();
        assert!(
            !ids.contains(&"doc-req:DOC-E:FR-005"),
            "broken 関連: {ids:?}"
        );
        assert!(
            !ids.contains(&"doc-req:DOC-D:FR-004"),
            "invalid 関連: {ids:?}"
        );
        assert!(
            !ids.contains(&"doc-req:DOC-GONE:FR-009"),
            "removed: {ids:?}"
        );
        assert!(
            !ids.contains(&"doc-req:DOC-C:FR-003"),
            "finding なし: {ids:?}"
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsはedge指定で絞り込み重複指定は1つにまとめる() {
        let targets = select(&[
            "test-req:tests/t::checks:FR-001",
            "doc-req:DOC-A:FR-001",
            "doc-req:DOC-A:FR-001",
        ])
        .unwrap();
        let ids: Vec<&str> = targets.iter().map(|t| t.edge_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["doc-req:DOC-A:FR-001", "test-req:tests/t::checks:FR-001"]
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsは対象外のidを指定するとnot_a_targetになる() {
        for bad in [
            "doc-req:DOC-C:FR-003",
            "doc-req:DOC-GONE:FR-009",
            "doc-req:DOC-E:FR-005",
            "doc-req:DOC-D:FR-004",
            "doc-req:NOPE:FR-404",
        ] {
            let err = select(&["doc-req:DOC-A:FR-001", bad]).unwrap_err();
            assert!(
                matches!(&err, JudgeError::NotATarget(id) if id == bad),
                "{bad} should be NotATarget, got {err:?}"
            );
        }
    }

    // [FR-041]
    #[test]
    fn build_jev_requestはdoc_reqエッジのstateとquestionsを組み立てる() {
        let i = input(
            "doc-req:DOC-A:FR-001",
            EdgeKind::DocReq,
            "DOC 本文",
            "REQ 本文",
        );
        let v = build_jev_request(&i);
        assert_eq!(v["model"], "jev-latest");
        assert_eq!(v["state"]["doc"], "DOC 本文");
        assert_eq!(v["state"]["req"], "REQ 本文");
        assert!(v["state"].get("test").is_none());
        let questions = v["questions"].as_object().unwrap();
        assert_eq!(questions.len(), 1);
        let q = &v["questions"]["consistent"];
        assert_eq!(q["type"], "noul");
        let instructions = q["instructions"].as_str().unwrap();
        assert!(instructions.contains("矛盾せず"), "{instructions}");
        assert!(instructions.contains("具体化"), "{instructions}");
        assert!(q["criteria"]["true"].as_str().unwrap().contains("具体化"));
        assert!(q["criteria"]["false"].as_str().unwrap().contains("矛盾"));
    }

    // [FR-041]
    #[test]
    fn build_jev_requestはtest_reqエッジのstateとquestionsを組み立てる() {
        let i = input(
            "test-req:tests/t::checks:FR-001",
            EdgeKind::TestReq,
            "fn checks() {}",
            "REQ 本文",
        );
        let v = build_jev_request(&i);
        assert_eq!(v["model"], "jev-latest");
        assert_eq!(v["state"]["test"], "fn checks() {}");
        assert_eq!(v["state"]["req"], "REQ 本文");
        assert!(v["state"].get("doc").is_none());
        assert_eq!(v["questions"].as_object().unwrap().len(), 1);
        let q = &v["questions"]["consistent"];
        assert_eq!(q["type"], "noul");
        let instructions = q["instructions"].as_str().unwrap();
        assert!(instructions.contains("検証して"), "{instructions}");
        assert!(q["criteria"]["true"].as_str().unwrap().contains("検証して"));
        assert!(q["criteria"]["false"]
            .as_str()
            .unwrap()
            .contains("検証していない"));
    }

    // [FR-042]
    #[test]
    fn parse_jev_responseは正常な応答からスコアを取り出す() {
        let body = r#"{"model":"jev-1.13.0","answers":{"consistent":{"type":"noul","noul":0.95}},"usage":{"input_tokens":1,"output_tokens":1}}"#;
        assert_eq!(parse_jev_response(body).unwrap(), 0.95);
    }

    // [FR-042]
    #[test]
    fn parse_jev_responseは0と1の境界値を受け入れる() {
        assert_eq!(parse_jev_response(&jev_body(0.0)).unwrap(), 0.0);
        assert_eq!(parse_jev_response(&jev_body(1.0)).unwrap(), 1.0);
    }

    // [FR-042]
    #[test]
    fn parse_jev_responseは範囲外_欠落_非数値_壊れたjsonを拒否する() {
        let bad = [
            jev_body(-0.1),
            jev_body(1.5),
            r#"{"answers":{}}"#.to_string(),
            r#"{"answers":{"consistent":{"type":"noul"}}}"#.to_string(),
            r#"{"answers":{"consistent":{"type":"noul","noul":"0.9"}}}"#.to_string(),
            r#"{"answers":{"consistent":{"type":"noul","noul":null}}}"#.to_string(),
            "{ not json".to_string(),
            String::new(),
        ];
        for body in bad {
            let err = parse_jev_response(&body).unwrap_err();
            assert!(
                matches!(err, JudgeError::InvalidJevResponse(_)),
                "{body}: {err:?}"
            );
        }
    }

    fn judge_one(score: f64, threshold: f64) -> Verdict {
        let inputs = vec![input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, "d", "r")];
        let body = jev_body(score);
        let jev = move |_: &serde_json::Value| Ok(body.clone());
        let result = judge_edges(&inputs, threshold, &jev).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].score, Some(score));
        result[0].verdict
    }

    // [FR-044]
    #[test]
    fn スコアが閾値ちょうどならconsistentになる() {
        assert_eq!(judge_one(0.9, 0.9), Verdict::Consistent);
    }

    // [FR-044]
    #[test]
    fn スコアが閾値未満ならreviewになる() {
        assert_eq!(judge_one(0.89, 0.9), Verdict::Review);
    }

    // [FR-044]
    #[test]
    fn judge_edgesはエッジid昇順で判定結果を返す() {
        let inputs = vec![
            input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, "d", "r"),
            input("doc-req:DOC-B:FR-002", EdgeKind::DocReq, "d", "r"),
        ];
        let scores = Cell::new(0usize);
        let jev = |_: &serde_json::Value| {
            let n = scores.get();
            scores.set(n + 1);
            Ok(jev_body(if n == 0 { 0.95 } else { 0.3 }))
        };
        let result = judge_edges(&inputs, 0.9, &jev).unwrap();
        assert_eq!(result[0].edge, "doc-req:DOC-A:FR-001");
        assert_eq!(result[0].verdict, Verdict::Consistent);
        assert_eq!(result[1].edge, "doc-req:DOC-B:FR-002");
        assert_eq!(result[1].verdict, Verdict::Review);
    }

    // [FR-043]
    #[test]
    fn jevがhttpエラーを返すとjudge_edgesはエラーになる() {
        let inputs = vec![input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, "d", "r")];
        let jev = |_: &serde_json::Value| Err(JudgeError::HttpStatus { status: 503 });
        let err = judge_edges(&inputs, 0.9, &jev).unwrap_err();
        assert!(
            matches!(err, JudgeError::HttpStatus { status: 503 }),
            "{err:?}"
        );
    }

    // [FR-048]
    #[test]
    fn 本文合計が上限を超えるとjevを呼ばずskippedになる() {
        let big = "a".repeat(MAX_BODY_BYTES);
        let inputs = vec![input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, &big, "b")];
        let calls = Cell::new(0usize);
        let jev = |_: &serde_json::Value| {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let result = judge_edges(&inputs, 0.9, &jev).unwrap();
        assert_eq!(result[0].verdict, Verdict::Skipped);
        assert_eq!(result[0].score, None);
        assert_eq!(calls.get(), 0);
    }

    // [FR-048]
    #[test]
    fn 本文合計が上限ちょうどならjevを呼ぶ() {
        let big = "a".repeat(MAX_BODY_BYTES - 3);
        // マルチバイト文字（3 バイト）を含めてバイト数で数えることも確かめる
        let inputs = vec![input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, &big, "あ")];
        let calls = Cell::new(0usize);
        let jev = |_: &serde_json::Value| {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let result = judge_edges(&inputs, 0.9, &jev).unwrap();
        assert_eq!(result[0].verdict, Verdict::Consistent);
        assert_eq!(calls.get(), 1);
    }

    fn prompt_fixture() -> (Vec<EdgeInput>, Vec<Judgment>) {
        let mut a = input(
            "doc-req:DOC-A:FR-001",
            EdgeKind::DocReq,
            "DOC-A-BODY",
            "REQ-A-BODY",
        );
        a.from_path = "docs/a.md".to_string();
        a.req_path = "req/a.md".to_string();
        let mut b = input(
            "doc-req:DOC-B:FR-002",
            EdgeKind::DocReq,
            "DOC-B-BODY",
            "REQ-B-BODY",
        );
        b.from_path = "docs/b.md".to_string();
        b.req_path = "req/b.md".to_string();
        let mut c = input(
            "test-req:tests/t::checks:FR-003",
            EdgeKind::TestReq,
            "TEST-C-BODY",
            "REQ-C-BODY",
        );
        c.from_path = "tests/t.rs".to_string();
        let mut d = input(
            "doc-req:DOC-D:FR-004",
            EdgeKind::DocReq,
            "DOC-D-BODY",
            "REQ-D-BODY",
        );
        d.from_path = "docs/d.md".to_string();
        let judgments = vec![
            judgment("doc-req:DOC-A:FR-001", Verdict::Consistent, Some(0.95)),
            judgment("doc-req:DOC-B:FR-002", Verdict::Review, Some(0.31)),
            Judgment {
                edge: "test-req:tests/t::checks:FR-003".to_string(),
                kind: EdgeKind::TestReq,
                verdict: Verdict::Review,
                score: Some(0.42),
            },
            judgment("doc-req:DOC-D:FR-004", Verdict::Skipped, None),
        ];
        (vec![a, b, c, d], judgments)
    }

    // [FR-046]
    #[test]
    fn プロンプトにはreviewのエッジの詳細が含まれる() {
        let (inputs, judgments) = prompt_fixture();
        let p = build_agent_prompt(&inputs, &judgments, false);
        for needle in [
            "doc-req:DOC-B:FR-002",
            "docs/b.md",
            "req/b.md",
            "DOC-B-BODY",
            "REQ-B-BODY",
            "0.31",
            "test-req:tests/t::checks:FR-003",
            "tests/t.rs",
            "TEST-C-BODY",
            "REQ-C-BODY",
            "0.42",
        ] {
            assert!(p.contains(needle), "{needle} not in prompt: {p}");
        }
        assert!(p.contains("doc-req") && p.contains("test-req"));
    }

    // [FR-046]
    #[test]
    fn プロンプトにはconsistentとskippedのエッジを含めない() {
        let (inputs, judgments) = prompt_fixture();
        let p = build_agent_prompt(&inputs, &judgments, false);
        assert!(!p.contains("doc-req:DOC-A:FR-001"), "{p}");
        assert!(!p.contains("DOC-A-BODY"), "{p}");
        assert!(!p.contains("doc-req:DOC-D:FR-004"), "{p}");
        assert!(!p.contains("DOC-D-BODY"), "{p}");
    }

    // [FR-046]
    #[test]
    fn プロンプトのエッジはid昇順に並ぶ() {
        let (mut inputs, mut judgments) = prompt_fixture();
        inputs.reverse();
        judgments.reverse();
        let p = build_agent_prompt(&inputs, &judgments, false);
        let pos_b = p.find("doc-req:DOC-B:FR-002").unwrap();
        let pos_c = p.find("test-req:tests/t::checks:FR-003").unwrap();
        assert!(pos_b < pos_c, "{p}");
    }

    // [FR-046]
    #[test]
    fn reviewが0件ならプロンプトは空文字列になる() {
        let (inputs, judgments) = prompt_fixture();
        let only_others: Vec<Judgment> = judgments
            .into_iter()
            .filter(|j| j.verdict != Verdict::Review)
            .collect();
        assert_eq!(build_agent_prompt(&inputs, &only_others, false), "");
    }

    // [FR-047]
    #[test]
    fn auto_reconcile指定ありならreconcileの実行指示を含む() {
        let (inputs, judgments) = prompt_fixture();
        let p = build_agent_prompt(&inputs, &judgments, true);
        assert!(p.contains("miwa reconcile --edge"), "{p}");
        assert!(p.contains("--reason"), "{p}");
        assert!(!p.contains("reconcile は実行しない"), "{p}");
    }

    // [FR-047]
    #[test]
    fn auto_reconcile指定なしならreconcileを実行しないと明記する() {
        let (inputs, judgments) = prompt_fixture();
        let p = build_agent_prompt(&inputs, &judgments, false);
        assert!(p.contains("reconcile は実行しない"), "{p}");
        assert!(!p.contains("miwa reconcile --edge"), "{p}");
    }

    // [FR-047]
    #[test]
    fn プロンプトは両モードで共通の判定基準と編集禁止を含む() {
        let (inputs, judgments) = prompt_fixture();
        for auto in [false, true] {
            let p = build_agent_prompt(&inputs, &judgments, auto);
            assert!(p.contains("編集しない"), "{p}");
            assert!(p.contains("矛盾"), "{p}");
            assert!(p.contains("検証"), "{p}");
            assert!(p.contains("理由"), "{p}");
        }
    }

    // [FR-049]
    #[test]
    fn render_textは1エッジ1行でverdictとスコアを小数2桁で表示する() {
        let judgments = vec![
            judgment("doc-req:DOC-A:FR-001", Verdict::Consistent, Some(0.94)),
            judgment("doc-req:DOC-B:FR-002", Verdict::Review, Some(0.31)),
            judgment("doc-req:DOC-C:FR-003", Verdict::Skipped, None),
        ];
        assert_eq!(
            render_text(&judgments),
            "consistent doc-req:DOC-A:FR-001 score=0.94\n\
             review doc-req:DOC-B:FR-002 score=0.31\n\
             skipped doc-req:DOC-C:FR-003 score=-\n"
        );
    }

    // [FR-049]
    #[test]
    fn render_textはスコアを小数第2位に丸めて表示する() {
        let judgments = vec![
            judgment("doc-req:DOC-A:FR-001", Verdict::Consistent, Some(1.0)),
            judgment("doc-req:DOC-B:FR-002", Verdict::Review, Some(0.0)),
            judgment("doc-req:DOC-C:FR-003", Verdict::Review, Some(0.3)),
        ];
        assert_eq!(
            render_text(&judgments),
            "consistent doc-req:DOC-A:FR-001 score=1.00\n\
             review doc-req:DOC-B:FR-002 score=0.00\n\
             review doc-req:DOC-C:FR-003 score=0.30\n"
        );
    }

    // [FR-049]
    #[test]
    fn render_textは0件なら対象なしを返す() {
        assert_eq!(render_text(&[]), "対象なし\n");
    }

    fn reconcile_fixture() -> (Lock, Vec<EdgeInput>, Vec<Judgment>) {
        let mut a = input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, "a", "r");
        a.from_hash = "sha256:a-from".to_string();
        a.to_hash = "sha256:a-to".to_string();
        let mut b = input("doc-req:DOC-B:FR-002", EdgeKind::DocReq, "b", "r");
        b.from_hash = "sha256:b-from".to_string();
        b.to_hash = "sha256:b-to".to_string();
        let mut c = input("doc-req:DOC-C:FR-003", EdgeKind::DocReq, "c", "r");
        c.from_hash = "sha256:c-from".to_string();
        c.to_hash = "sha256:c-to".to_string();
        let judgments = vec![
            judgment("doc-req:DOC-A:FR-001", Verdict::Consistent, Some(0.95)),
            judgment("doc-req:DOC-B:FR-002", Verdict::Review, Some(0.3)),
            judgment("doc-req:DOC-C:FR-003", Verdict::Skipped, None),
        ];
        let mut lock = Lock::default();
        lock.edges.insert(
            "doc-req:DOC-B:FR-002".to_string(),
            lock_entry("sha256:old-b", "sha256:old-b-to"),
        );
        (lock, vec![a, b, c], judgments)
    }

    // [FR-051]
    #[test]
    fn apply_auto_reconcileはconsistentのエッジだけをjevの合意として記録する() {
        let (lock, inputs, judgments) = reconcile_fixture();
        let new_lock = apply_auto_reconcile(&lock, &inputs, &judgments, "2026-02-03T04:05:06Z");
        let e = &new_lock.edges["doc-req:DOC-A:FR-001"];
        assert_eq!(e.by, "jev");
        assert_eq!(e.score, Some(0.95));
        assert_eq!(e.reason, None);
        assert_eq!(e.at, "2026-02-03T04:05:06Z");
        assert_eq!(e.from, "sha256:a-from");
        assert_eq!(e.to, "sha256:a-to");
    }

    // [FR-051]
    #[test]
    fn apply_auto_reconcileはreviewとskippedを記録せず既存エントリを変えない() {
        let (lock, inputs, judgments) = reconcile_fixture();
        let new_lock = apply_auto_reconcile(&lock, &inputs, &judgments, "2026-02-03T04:05:06Z");
        assert!(!new_lock.edges.contains_key("doc-req:DOC-C:FR-003"));
        assert_eq!(
            new_lock.edges["doc-req:DOC-B:FR-002"],
            lock.edges["doc-req:DOC-B:FR-002"]
        );
        assert_eq!(new_lock.edges.len(), 2);
    }

    // [FR-051]
    #[test]
    fn apply_auto_reconcileは元のロックを変更しない() {
        let (lock, inputs, judgments) = reconcile_fixture();
        let before = lock.clone();
        let _ = apply_auto_reconcile(&lock, &inputs, &judgments, "2026-02-03T04:05:06Z");
        assert_eq!(lock, before);
    }

    fn cmd(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    // [FR-045]
    #[test]
    fn run_agentはプロンプトを標準入力で渡し作業ディレクトリで実行する() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("stdin.txt");
        let command = cmd(&[
            "sh",
            "-c",
            &format!("cat > {}; pwd > cwd.txt", out.display()),
        ]);
        run_agent(&command, "プロンプト本文\n2行目", dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "プロンプト本文\n2行目"
        );
        let cwd = std::fs::read_to_string(dir.path().join("cwd.txt")).unwrap();
        assert_eq!(
            std::fs::canonicalize(cwd.trim()).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    // [FR-045]
    #[test]
    fn run_agentは非0終了をagent_failedにする() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_agent(&cmd(&["sh", "-c", "exit 3"]), "p", dir.path()).unwrap_err();
        assert!(matches!(err, JudgeError::AgentFailed(3)), "{err:?}");
    }

    // [FR-045]
    #[test]
    fn run_agentは存在しないコマンドをagent_spawnにする() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_agent(&cmd(&["miwa-no-such-command-xyz"]), "p", dir.path()).unwrap_err();
        assert!(matches!(err, JudgeError::AgentSpawn(_)), "{err:?}");
    }
}
