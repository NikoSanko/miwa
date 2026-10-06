//! Text / JSON rendering of findings.

fn kind_label(kind: crate::check::FindingKind) -> &'static str {
    use crate::check::FindingKind::*;
    match kind {
        Drift => "drift",
        New => "new",
        Removed => "removed",
        Broken => "broken",
        Orphan => "orphan",
        NoTest => "no-test",
        Invalid => "invalid",
        Index => "index",
    }
}

/// One line per finding. `"no findings\n"` if empty.
pub fn render_text(findings: &[crate::check::Finding]) -> String {
    if findings.is_empty() {
        return "no findings\n".to_string();
    }
    let mut out = String::new();
    for f in findings {
        out.push_str(&format!(
            "{} {:?} {} {:?} {}\n",
            kind_label(f.kind),
            f.target_kind,
            f.target,
            f.reason,
            f.detail
        ));
    }
    out
}

/// `{"findings":[...]}`, pretty-printed.
pub fn render_json(findings: &[crate::check::Finding]) -> String {
    #[derive(serde::Serialize)]
    struct Wrapper<'a> {
        findings: &'a [crate::check::Finding],
    }
    serde_json::to_string_pretty(&Wrapper { findings }).expect("findings serialize to json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{Finding, FindingKind};
    use crate::model::{ReasonCode, TargetKind};

    fn sample_findings() -> Vec<Finding> {
        vec![
            Finding {
                kind: FindingKind::Drift,
                target_kind: TargetKind::Edge,
                target: "doc-req:DOC-A:FR-001".to_string(),
                reason: ReasonCode::DocChanged,
                detail: "doc changed".to_string(),
            },
            Finding {
                kind: FindingKind::NoTest,
                target_kind: TargetKind::Req,
                target: "FR-002".to_string(),
                reason: ReasonCode::ReqWithoutTest,
                detail: "no test found".to_string(),
            },
        ]
    }

    // [FR-032]
    #[test]
    fn render_textはfindingなしなら固定文言になる() {
        assert_eq!(render_text(&[]), "no findings\n");
    }

    // [FR-032]
    #[test]
    fn render_textは1行1findingでkindとtargetを含む() {
        let findings = sample_findings();
        let text = render_text(&findings);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), findings.len());
        assert!(lines[0].contains("doc-req:DOC-A:FR-001"));
        assert!(lines[0].to_lowercase().contains("drift"));
        assert!(lines[1].contains("FR-002"));
        assert!(lines[1].to_lowercase().contains("no")); // no-test / NoTest 系の文字列を含む
    }

    // [FR-032]
    #[test]
    fn render_jsonはfindings配列にkind_target_kind_target_reason_detailを持つ() {
        let findings = sample_findings();
        let json = render_json(&findings);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let arr = value
            .get("findings")
            .and_then(|v| v.as_array())
            .expect("findings array");
        assert_eq!(arr.len(), 2);
        for (i, item) in arr.iter().enumerate() {
            assert!(item.get("kind").is_some(), "missing kind at {i}");
            assert!(
                item.get("target_kind").is_some(),
                "missing target_kind at {i}"
            );
            assert!(item.get("target").is_some(), "missing target at {i}");
            assert!(item.get("reason").is_some(), "missing reason at {i}");
            assert!(item.get("detail").is_some(), "missing detail at {i}");
        }
        assert_eq!(arr[0]["kind"], serde_json::json!("Drift"));
        assert_eq!(arr[0]["target"], serde_json::json!("doc-req:DOC-A:FR-001"));
        assert_eq!(arr[0]["reason"], serde_json::json!("DocChanged"));
        assert_eq!(arr[1]["kind"], serde_json::json!("NoTest"));
    }
}
