//! `judge`: semantic judgment of drifted / new edges via Jev.

use crate::model::EdgeKind;

pub const DEFAULT_MAX_BODY_BYTES: usize = 200_000;
pub const JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const JEV_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
/// テスト専用: 設定されていれば Jev の endpoint を上書きする
pub const JEV_ENDPOINT_OVERRIDE_ENV: &str = "MIWA_JEV_ENDPOINT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// スコアが閾値以上
    Consistent,
    /// スコアが閾値未満。判断は呼び出し元（人間または呼び出し側のエージェント）に委ねる
    Review,
    /// 本文が大きすぎて Jev を呼ばなかった
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeInput {
    pub edge_id: String,
    pub kind: EdgeKind,
    pub from_text: String,
    /// The req body for DocReq / TestReq edges; the node body for Desc edges.
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
}

/// drift / new のエッジを対象に EdgeInput を組む（エッジ ID 昇順）。requested が非空ならその ID だけ（重複は 1 つに）。対象外の ID は NotATarget
pub fn select_targets(
    graph: &crate::model::Graph,
    findings: &[crate::check::Finding],
    requested: &[String],
) -> Result<Vec<EdgeInput>, JudgeError> {
    use crate::check::FindingKind;
    use crate::model::TargetKind;
    use std::collections::BTreeSet;

    use crate::model::NodeKind;

    let invalid: BTreeSet<(TargetKind, &str)> = graph
        .invalid
        .iter()
        .map(|i| (i.target_kind, i.target.as_str()))
        .collect();
    let broken_from: BTreeSet<&crate::model::NodeKey> =
        graph.broken.iter().map(|b| &b.from).collect();
    let is_invalid = |key: &crate::model::NodeKey| {
        let kind = match key.kind {
            NodeKind::Doc => TargetKind::Doc,
            NodeKind::Req => TargetKind::Req,
            NodeKind::Test => TargetKind::Test,
        };
        invalid.contains(&(kind, key.id.as_str()))
    };

    let mut targets: BTreeSet<&str> = BTreeSet::new();
    for f in findings {
        if matches!(f.kind, FindingKind::Drift | FindingKind::New)
            && f.target_kind == TargetKind::Edge
        {
            targets.insert(f.target.as_str());
        }
    }

    let mut candidates = Vec::new();
    for id in targets {
        let Some(edge) = graph.edges.get(id) else {
            continue;
        };
        if is_invalid(&edge.from) || is_invalid(&edge.to) || broken_from.contains(&edge.from) {
            continue;
        }
        let (Some(from), Some(to)) = (graph.nodes.get(&edge.from), graph.nodes.get(&edge.to))
        else {
            continue;
        };
        candidates.push((edge, from, to));
    }

    if !requested.is_empty() {
        let wanted: BTreeSet<&str> = requested.iter().map(String::as_str).collect();
        for id in &wanted {
            if !candidates.iter().any(|(e, _, _)| e.id == *id) {
                return Err(JudgeError::NotATarget((*id).to_string()));
            }
        }
        candidates.retain(|(e, _, _)| wanted.contains(e.id.as_str()));
    }

    Ok(candidates
        .into_iter()
        .map(|(edge, from, to)| {
            let (from_text, req_text) = match edge.kind {
                EdgeKind::DocReq | EdgeKind::TestReq => (from.content.clone(), to.content.clone()),
                EdgeKind::Desc => (from.description_text().to_string(), from.content.clone()),
            };
            let (from_hash, to_hash) = crate::model::edge_hashes(graph, edge);
            EdgeInput {
                edge_id: edge.id.clone(),
                kind: edge.kind,
                from_text,
                req_text,
                from_hash,
                to_hash,
            }
        })
        .collect())
}

pub fn build_jev_request(input: &EdgeInput) -> serde_json::Value {
    let (state, instructions, true_c, false_c) = match input.kind {
        EdgeKind::DocReq => (
            serde_json::json!({"doc": input.from_text, "req": input.req_text}),
            "`req` は `doc` の該当部分と矛盾せず、その部分を具体化したシステム要件になっているか",
            "req が扱う事項について doc と矛盾がなく、doc の記述を具体化している",
            "req が doc と矛盾している、または doc に対応する記述がない",
        ),
        EdgeKind::TestReq => (
            serde_json::json!({"test": input.from_text, "req": input.req_text}),
            "`test` は `req` に書かれた振る舞いを検証しており、`req` と矛盾する期待値を持たないか",
            "test が req の振る舞いを検証しており、req と矛盾する期待値がない",
            "test が req の振る舞いを検証していない、または req と矛盾する期待値を持つ",
        ),
        EdgeKind::Desc => (
            serde_json::json!({"description": input.from_text, "body": input.req_text}),
            "`description` は `body` の内容を正しく要約しているか",
            "description が body の主題と内容を正しく表しており、body にない事柄を述べていない",
            "description が body の内容と食い違っている、または body の主題を表していない",
        ),
    };
    serde_json::json!({
        "model": "jev-latest",
        "state": state,
        "questions": {
            "consistent": {
                "type": "noul",
                "instructions": instructions,
                "criteria": {"true": true_c, "false": false_c},
            }
        }
    })
}

pub fn parse_jev_response(body: &str) -> Result<f64, JudgeError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| JudgeError::InvalidJevResponse(format!("not valid JSON: {e}")))?;
    let score = value
        .pointer("/answers/consistent/noul")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| {
            JudgeError::InvalidJevResponse("answers.consistent.noul is missing".to_string())
        })?;
    if !score.is_finite() || !(0.0..=1.0).contains(&score) {
        return Err(JudgeError::InvalidJevResponse(format!(
            "score {score} is outside 0..=1"
        )));
    }
    // -0.0 を 0.0 に正規化する（`score=-0.00` を出さない）
    Ok(if score == 0.0 { 0.0 } else { score })
}

pub type HttpCall<'a> = &'a dyn Fn(&serde_json::Value) -> Result<String, JudgeError>;

/// 各エッジを判定。本文合計が max_body_bytes 超なら Jev を呼ばず Skipped（score None）。score >= threshold で Consistent、未満で Review。順序はエッジ ID 昇順。Jev のエラーはそのまま Err
pub fn judge_edges(
    inputs: &[EdgeInput],
    threshold: f64,
    max_body_bytes: usize,
    jev: HttpCall,
) -> Result<Vec<Judgment>, JudgeError> {
    let mut sorted: Vec<&EdgeInput> = inputs.iter().collect();
    sorted.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
    let mut out = Vec::new();
    for input in sorted {
        let (verdict, score) = if input.from_text.len() + input.req_text.len() > max_body_bytes {
            (Verdict::Skipped, None)
        } else {
            let body = jev(&build_jev_request(input))?;
            let score = parse_jev_response(&body)?;
            let verdict = if score >= threshold {
                Verdict::Consistent
            } else {
                Verdict::Review
            };
            (verdict, Some(score))
        };
        out.push(Judgment {
            edge: input.edge_id.clone(),
            kind: input.kind,
            verdict,
            score,
        });
    }
    Ok(out)
}

/// Consistent のエッジだけを by: "jev" + score で記録した新しい Lock を返す（元の Lock は変更しない）
pub fn apply_auto_reconcile(
    lock: &crate::lock::Lock,
    inputs: &[EdgeInput],
    judgments: &[Judgment],
    now: &str,
) -> crate::lock::Lock {
    let mut new_lock = lock.clone();
    for j in judgments
        .iter()
        .filter(|j| j.verdict == Verdict::Consistent)
    {
        if let Some(input) = inputs.iter().find(|i| i.edge_id == j.edge) {
            new_lock.edges.insert(
                j.edge.clone(),
                crate::lock::LockEntry {
                    from: input.from_hash.clone(),
                    to: input.to_hash.clone(),
                    by: "jev".to_string(),
                    reason: None,
                    score: j.score,
                    at: now.to_string(),
                },
            );
        }
    }
    new_lock
}

/// 1 行 1 エッジ `<consistent|review|skipped> <edge id> score=<0.00〜1.00 | ->`。0 件なら "対象なし\n"
pub fn render_text(judgments: &[Judgment]) -> String {
    if judgments.is_empty() {
        return "対象なし\n".to_string();
    }
    let mut out = String::new();
    for j in judgments {
        let verdict = match j.verdict {
            Verdict::Consistent => "consistent",
            Verdict::Review => "review",
            Verdict::Skipped => "skipped",
        };
        let score = j.score.map_or("-".to_string(), |s| format!("{s:.2}"));
        out.push_str(&format!("{verdict} {} score={score}\n", j.edge));
    }
    out
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
                description: None,
            },
        );
    }

    fn add_edge(graph: &mut Graph, kind: EdgeKind, from: &str, to: &str) -> String {
        let id = crate::model::edge_id(kind, from, to);
        let from_kind = match kind {
            EdgeKind::DocReq => NodeKind::Doc,
            EdgeKind::TestReq => NodeKind::Test,
            EdgeKind::Desc => unreachable!("use add_desc_edge"),
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
        select_targets(&graph, &findings, &requested)
    }

    fn input(id: &str, kind: EdgeKind, from_text: &str, req_text: &str) -> EdgeInput {
        EdgeInput {
            edge_id: id.to_string(),
            kind,
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
        let result = judge_edges(&inputs, threshold, DEFAULT_MAX_BODY_BYTES, &jev).unwrap();
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
            input("doc-req:DOC-B:FR-002", EdgeKind::DocReq, "dB", "r"),
            input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, "dA", "r"),
        ];
        let scores = Cell::new(0usize);
        let order = std::cell::RefCell::new(Vec::new());
        let jev = |v: &serde_json::Value| {
            order
                .borrow_mut()
                .push(v["state"]["doc"].as_str().unwrap().to_string());
            let n = scores.get();
            scores.set(n + 1);
            Ok(jev_body(if n == 0 { 0.95 } else { 0.3 }))
        };
        let result = judge_edges(&inputs, 0.9, DEFAULT_MAX_BODY_BYTES, &jev).unwrap();
        assert_eq!(*order.borrow(), vec!["dA".to_string(), "dB".to_string()]);
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
        let err = judge_edges(&inputs, 0.9, DEFAULT_MAX_BODY_BYTES, &jev).unwrap_err();
        assert!(
            matches!(err, JudgeError::HttpStatus { status: 503 }),
            "{err:?}"
        );
    }

    // [FR-045]
    #[test]
    fn 本文合計が上限を超えるとjevを呼ばずskippedになる() {
        let big = "a".repeat(DEFAULT_MAX_BODY_BYTES);
        let inputs = vec![input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, &big, "b")];
        let calls = Cell::new(0usize);
        let jev = |_: &serde_json::Value| {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let result = judge_edges(&inputs, 0.9, DEFAULT_MAX_BODY_BYTES, &jev).unwrap();
        assert_eq!(result[0].verdict, Verdict::Skipped);
        assert_eq!(result[0].score, None);
        assert_eq!(calls.get(), 0);
    }

    // [FR-045]
    #[test]
    fn 本文合計が上限ちょうどならjevを呼ぶ() {
        let big = "a".repeat(DEFAULT_MAX_BODY_BYTES - 3);
        // マルチバイト文字（3 バイト）を含めてバイト数で数えることも確かめる
        let inputs = vec![input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, &big, "あ")];
        let calls = Cell::new(0usize);
        let jev = |_: &serde_json::Value| {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let result = judge_edges(&inputs, 0.9, DEFAULT_MAX_BODY_BYTES, &jev).unwrap();
        assert_eq!(result[0].verdict, Verdict::Consistent);
        assert_eq!(calls.get(), 1);
    }

    // [FR-045]
    #[test]
    fn 上限は引数で指定でき小さい上限なら通常サイズでもskippedになる() {
        let inputs = vec![input(
            "doc-req:DOC-A:FR-001",
            EdgeKind::DocReq,
            "aaaaaa",
            "bbbbb",
        )];
        let calls = Cell::new(0usize);
        let jev = |_: &serde_json::Value| {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let result = judge_edges(&inputs, 0.9, 10, &jev).unwrap();
        assert_eq!(result[0].verdict, Verdict::Skipped);
        assert_eq!(calls.get(), 0);
        let result = judge_edges(&inputs, 0.9, 11, &jev).unwrap();
        assert_eq!(result[0].verdict, Verdict::Consistent);
        assert_eq!(calls.get(), 1);
    }

    // [FR-046]
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

    // [FR-046]
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

    // [FR-046]
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

    // [FR-048]
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

    // [FR-048]
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

    // [FR-048]
    #[test]
    fn apply_auto_reconcileは元のロックを変更しない() {
        let (lock, inputs, judgments) = reconcile_fixture();
        let before = lock.clone();
        let _ = apply_auto_reconcile(&lock, &inputs, &judgments, "2026-02-03T04:05:06Z");
        assert_eq!(lock, before);
    }

    // [FR-042]
    #[test]
    fn parse_jev_responseは負のゼロを0に正規化する() {
        let score = parse_jev_response(&jev_body(-0.0)).unwrap();
        assert!(score.is_sign_positive(), "{score}");
        let out = render_text(&[judgment(
            "doc-req:DOC-A:FR-001",
            Verdict::Review,
            Some(score),
        )]);
        assert_eq!(out, "review doc-req:DOC-A:FR-001 score=0.00\n");
    }

    // [FR-041]
    #[test]
    fn jevの質問文言はreqの文言と完全一致する() {
        let d = build_jev_request(&input("doc-req:DOC-A:FR-001", EdgeKind::DocReq, "d", "r"));
        let q = &d["questions"]["consistent"];
        assert_eq!(
            q["instructions"],
            "`req` は `doc` の該当部分と矛盾せず、その部分を具体化したシステム要件になっているか"
        );
        assert_eq!(
            q["criteria"]["true"],
            "req が扱う事項について doc と矛盾がなく、doc の記述を具体化している"
        );
        assert_eq!(
            q["criteria"]["false"],
            "req が doc と矛盾している、または doc に対応する記述がない"
        );
        let t = build_jev_request(&input("test-req:t:FR-001", EdgeKind::TestReq, "t", "r"));
        let q = &t["questions"]["consistent"];
        assert_eq!(
            q["instructions"],
            "`test` は `req` に書かれた振る舞いを検証しており、`req` と矛盾する期待値を持たないか"
        );
        assert_eq!(
            q["criteria"]["true"],
            "test が req の振る舞いを検証しており、req と矛盾する期待値がない"
        );
        assert_eq!(
            q["criteria"]["false"],
            "test が req の振る舞いを検証していない、または req と矛盾する期待値を持つ"
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsはinvalidの種別と端点の種別が一致するときだけ除外する() {
        let build = |invalid_kind: TargetKind| {
            let mut graph = Graph::default();
            add_node(
                &mut graph,
                NodeKind::Doc,
                "X",
                "docs/x.md",
                "doc",
                "sha256:d",
            );
            add_node(
                &mut graph,
                NodeKind::Req,
                "X",
                "req/x.md",
                "req",
                "sha256:r",
            );
            add_node(
                &mut graph,
                NodeKind::Test,
                "tests/t::c",
                "tests/t.rs",
                "fn c() {}",
                "sha256:t",
            );
            let id = add_edge(&mut graph, EdgeKind::TestReq, "tests/t::c", "X");
            graph.invalid.push(Invalid {
                target_kind: invalid_kind,
                target: "X".to_string(),
                reason: ReasonCode::DuplicateId,
                detail: "dup".to_string(),
            });
            let findings = check(&graph, &Lock::default());
            (select_targets(&graph, &findings, &[]).unwrap(), id)
        };
        let (targets, id) = build(TargetKind::Doc);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].edge_id, id);
        let (targets, _) = build(TargetKind::Req);
        assert!(targets.is_empty(), "{targets:?}");
    }

    /// Adds a Doc / Req node with a description and its self-loop `desc:doc:<id>` / `desc:req:<id>` edge.
    fn add_desc_node(
        graph: &mut Graph,
        kind: NodeKind,
        id: &str,
        description: Option<&str>,
        content: &str,
        hash: &str,
    ) -> String {
        add_node(graph, kind, id, &format!("docs/{id}.md"), content, hash);
        let key = NodeKey {
            kind,
            id: id.to_string(),
        };
        graph.nodes.get_mut(&key).unwrap().description = description.map(str::to_string);
        add_desc_edge(graph, kind, id)
    }

    fn add_desc_edge(graph: &mut Graph, kind: NodeKind, id: &str) -> String {
        let key = NodeKey {
            kind,
            id: id.to_string(),
        };
        let edge_id = crate::model::desc_edge_id(&key);
        graph.edges.insert(
            edge_id.clone(),
            Edge {
                id: edge_id.clone(),
                kind: EdgeKind::Desc,
                from: key.clone(),
                to: key,
            },
        );
        edge_id
    }

    fn select_in(
        graph: &Graph,
        lock: &Lock,
        requested: &[&str],
    ) -> Result<Vec<EdgeInput>, JudgeError> {
        let findings = check(graph, lock);
        let requested: Vec<String> = requested.iter().map(|s| s.to_string()).collect();
        select_targets(graph, &findings, &requested)
    }

    // [FR-038]
    #[test]
    fn select_targetsはdescのdriftを対象にし要約と本文を詰める() {
        let mut graph = Graph::default();
        add_desc_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-A",
            Some("新しい要約"),
            "doc 本文",
            "sha256:body",
        );
        let mut lock = Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-A".to_string(),
            lock_entry(&crate::hash::content_hash("古い要約"), "sha256:body"),
        );
        let targets = select_in(&graph, &lock, &[]).unwrap();
        assert_eq!(
            targets,
            vec![EdgeInput {
                edge_id: "desc:doc:DOC-A".to_string(),
                kind: EdgeKind::Desc,
                from_text: "新しい要約".to_string(),
                req_text: "doc 本文".to_string(),
                from_hash: crate::hash::content_hash("新しい要約"),
                to_hash: "sha256:body".to_string(),
            }]
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsはdescのnewを対象にし要約が無ければfrom_textを空文字列にする() {
        let mut graph = Graph::default();
        add_desc_node(
            &mut graph,
            NodeKind::Req,
            "FR-001",
            None,
            "req 本文",
            "sha256:r",
        );
        let targets = select_in(&graph, &Lock::default(), &[]).unwrap();
        assert_eq!(targets.len(), 1, "{targets:?}");
        assert_eq!(targets[0].edge_id, "desc:req:FR-001");
        assert_eq!(targets[0].kind, EdgeKind::Desc);
        assert_eq!(targets[0].from_text, "");
        assert_eq!(targets[0].req_text, "req 本文");
        assert_eq!(targets[0].from_hash, crate::hash::content_hash(""));
        assert_eq!(targets[0].to_hash, "sha256:r");
    }

    // [FR-038]
    #[test]
    fn select_targetsはdescエッジもエッジid昇順に並べ_descがdoc_reqより先になる() {
        let (mut graph, lock) = mixed_fixture();
        add_desc_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-Z",
            Some("要約"),
            "doc z",
            "sha256:dz",
        );
        let findings = check(&graph, &lock);
        let targets = select_targets(&graph, &findings, &[]).unwrap();
        let ids: Vec<&str> = targets.iter().map(|t| t.edge_id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "desc:doc:DOC-Z",
                "doc-req:DOC-A:FR-001",
                "doc-req:DOC-B:FR-002",
                "test-req:tests/t::checks:FR-001",
            ]
        );
    }

    // [FR-038]
    #[test]
    fn select_targetsはedge指定でdescエッジを選べる() {
        let mut graph = Graph::default();
        add_desc_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-A",
            Some("要約"),
            "本文",
            "sha256:a",
        );
        add_desc_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-B",
            Some("要約"),
            "本文",
            "sha256:b",
        );
        let targets = select_in(
            &graph,
            &Lock::default(),
            &["desc:doc:DOC-B", "desc:doc:DOC-B"],
        )
        .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].edge_id, "desc:doc:DOC-B");
    }

    // [FR-038]
    #[test]
    fn select_targetsはfindingのないdescエッジを対象にせずedge指定はnot_a_targetになる() {
        let mut graph = Graph::default();
        add_desc_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-A",
            Some("要約"),
            "本文",
            "sha256:a",
        );
        let mut lock = Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-A".to_string(),
            lock_entry(&crate::hash::content_hash("要約"), "sha256:a"),
        );
        assert!(select_in(&graph, &lock, &[]).unwrap().is_empty());
        assert!(matches!(
            select_in(&graph, &lock, &["desc:doc:DOC-A"]),
            Err(JudgeError::NotATarget(_))
        ));
    }

    // [FR-038]
    #[test]
    fn select_targetsはロックにのみ存在するdescエッジのedge指定をnot_a_targetにする() {
        let mut lock = Lock::default();
        lock.edges.insert(
            "desc:doc:DOC-X".to_string(),
            lock_entry(&crate::hash::content_hash("要約"), "sha256:x"),
        );
        assert!(matches!(
            select_in(&Graph::default(), &lock, &["desc:doc:DOC-X"]),
            Err(JudgeError::NotATarget(_))
        ));
    }

    // [FR-038]
    #[test]
    fn select_targetsはリンク切れを持つdocのdescエッジを対象にしない() {
        let mut graph = Graph::default();
        add_desc_node(
            &mut graph,
            NodeKind::Doc,
            "DOC-A",
            Some("要約"),
            "本文",
            "sha256:a",
        );
        graph.broken.push(Broken {
            from: NodeKey {
                kind: NodeKind::Doc,
                id: "DOC-A".to_string(),
            },
            target: "../req/missing.md".to_string(),
            reason: ReasonCode::MissingReq,
            detail: "missing".to_string(),
        });
        assert!(select_in(&graph, &Lock::default(), &[]).unwrap().is_empty());
        assert!(matches!(
            select_in(&graph, &Lock::default(), &["desc:doc:DOC-A"]),
            Err(JudgeError::NotATarget(_))
        ));
    }

    // [FR-038]
    #[test]
    fn select_targetsはinvalidに関わるノードのdescエッジを対象にしない() {
        let mut graph = Graph::default();
        add_desc_node(
            &mut graph,
            NodeKind::Req,
            "FR-001",
            Some("要約"),
            "本文",
            "sha256:r",
        );
        graph.invalid.push(Invalid {
            target_kind: TargetKind::Req,
            target: "FR-001".to_string(),
            reason: ReasonCode::DuplicateId,
            detail: "dup".to_string(),
        });
        assert!(select_in(&graph, &Lock::default(), &[]).unwrap().is_empty());
    }

    // [FR-041]
    #[test]
    fn build_jev_requestはdescエッジのstateと質問文言を組み立てる() {
        let i = input("desc:doc:DOC-A", EdgeKind::Desc, "要約です", "本文です");
        let v = build_jev_request(&i);
        assert_eq!(v["model"], "jev-latest");
        assert_eq!(
            v["state"],
            serde_json::json!({"description": "要約です", "body": "本文です"})
        );
        assert_eq!(v["questions"].as_object().unwrap().len(), 1);
        let q = &v["questions"]["consistent"];
        assert_eq!(q["type"], "noul");
        assert_eq!(
            q["instructions"],
            "`description` は `body` の内容を正しく要約しているか"
        );
        assert_eq!(
            q["criteria"]["true"],
            "description が body の主題と内容を正しく表しており、body にない事柄を述べていない"
        );
        assert_eq!(
            q["criteria"]["false"],
            "description が body の内容と食い違っている、または body の主題を表していない"
        );
    }

    // [FR-041]
    #[test]
    fn build_jev_requestはdescで要約が空でもdescriptionキーを空文字列で送る() {
        let i = input("desc:doc:DOC-A", EdgeKind::Desc, "", "本文です");
        let v = build_jev_request(&i);
        assert_eq!(v["state"]["description"], "");
        assert_eq!(v["state"]["body"], "本文です");
    }

    // [FR-045]
    #[test]
    fn descエッジも要約と本文の合計が上限を超えるとjevを呼ばずskippedになる() {
        let calls = Cell::new(0);
        let jev = |_: &serde_json::Value| -> Result<String, JudgeError> {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let inputs = vec![input("desc:doc:DOC-A", EdgeKind::Desc, "12345", "123456")];
        let out = judge_edges(&inputs, 0.9, 10, &jev).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, EdgeKind::Desc);
        assert_eq!(out[0].verdict, Verdict::Skipped);
        assert_eq!(out[0].score, None);
        assert_eq!(calls.get(), 0);
    }

    // [FR-045]
    #[test]
    fn descエッジの要約と本文の合計が上限ちょうどならjevを呼ぶ() {
        let calls = Cell::new(0);
        let jev = |_: &serde_json::Value| -> Result<String, JudgeError> {
            calls.set(calls.get() + 1);
            Ok(jev_body(0.95))
        };
        let inputs = vec![input("desc:doc:DOC-A", EdgeKind::Desc, "12345", "12345")];
        let out = judge_edges(&inputs, 0.9, 10, &jev).unwrap();
        assert_eq!(out[0].verdict, Verdict::Consistent);
        assert_eq!(calls.get(), 1);
    }

    // [FR-048]
    #[test]
    fn apply_auto_reconcileはdescエッジの要約と本文のハッシュをjevの合意として記録する() {
        let mut i = input("desc:doc:DOC-A", EdgeKind::Desc, "要約", "本文");
        i.from_hash = "sha256:desc-from".to_string();
        i.to_hash = "sha256:desc-to".to_string();
        let judgments = vec![Judgment {
            edge: "desc:doc:DOC-A".to_string(),
            kind: EdgeKind::Desc,
            verdict: Verdict::Consistent,
            score: Some(0.97),
        }];
        let new_lock =
            apply_auto_reconcile(&Lock::default(), &[i], &judgments, "2026-02-03T04:05:06Z");
        let e = &new_lock.edges["desc:doc:DOC-A"];
        assert_eq!(e.by, "jev");
        assert_eq!(e.score, Some(0.97));
        assert_eq!(e.from, "sha256:desc-from");
        assert_eq!(e.to, "sha256:desc-to");
        assert_eq!(e.at, "2026-02-03T04:05:06Z");
    }
}
