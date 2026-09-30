//! YAML front matter parsing for doc / req markdown files.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    pub kind: Option<String>,
    pub id: Option<String>,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FrontMatterError {
    #[error("invalid yaml: {0}")]
    InvalidYaml(String),
    #[error("unterminated front matter")]
    Unterminated,
}

/// Parse the front matter of `text`. Returns `Ok(None)` if `text` does not start
/// with a `---` line.
pub fn parse(text: &str) -> Result<Option<FrontMatter>, FrontMatterError> {
    let _ = text;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // [FR-002]
    #[test]
    fn id_だけを変えても本文bodyは変わらない() {
        let before = "---\ntype: Requirement\nid: FR-001\n---\n本文です。\n";
        let after = "---\ntype: Requirement\nid: FR-002\n---\n本文です。\n";
        let fm_before = parse(before).unwrap().unwrap();
        let fm_after = parse(after).unwrap().unwrap();
        assert_eq!(fm_before.body, fm_after.body);
        assert_ne!(fm_before.id, fm_after.id);
    }

    // [FR-002]
    #[test]
    fn bodyにはフロントマターのブロックが含まれない() {
        let text = "---\ntype: Requirement\nid: FR-001\n---\n本文です。\n";
        let fm = parse(text).unwrap().unwrap();
        assert!(!fm.body.contains("id: FR-001"));
        assert!(!fm.body.contains("type: Requirement"));
        assert!(fm.body.contains("本文です。"));
    }

    // [FR-005]
    #[test]
    fn ハイフン3つで始まらないテキストはnoneになる() {
        let text = "# ただのMarkdown\n本文です。\n";
        assert_eq!(parse(text).unwrap(), None);
    }

    // [FR-005]
    #[test]
    fn typeフィールドがない場合はkindがnoneになる() {
        let text = "---\nid: FR-001\n---\n本文です。\n";
        let fm = parse(text).unwrap().unwrap();
        assert_eq!(fm.kind, None);
    }
}
