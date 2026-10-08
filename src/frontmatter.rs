//! YAML front matter parsing for doc / req markdown files.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    pub kind: Option<String>,
    pub id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FrontMatterError {
    #[error("invalid yaml: {0}")]
    InvalidYaml(String),
    #[error("unterminated front matter")]
    Unterminated,
}

/// Splits `text` into the front matter block (the lines between the opening
/// and closing `---`, without the fences) and the body, without parsing the
/// YAML. Returns `Ok(None)` if the first line is not `---` (the whole text is
/// the body), and `Err(Unterminated)` if no closing `---` line follows.
/// A trailing CR on the fence lines is ignored.
pub fn split(text: &str) -> Result<Option<(&str, &str)>, FrontMatterError> {
    let first_end = text.find('\n').unwrap_or(text.len());
    if text[..first_end].trim_end_matches('\r') != "---" {
        return Ok(None);
    }
    if first_end == text.len() {
        return Err(FrontMatterError::Unterminated);
    }
    let yaml_start = first_end + 1;
    let mut pos = yaml_start;
    loop {
        let end = text[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(text.len());
        if text[pos..end].trim_end_matches('\r') == "---" {
            let yaml = if pos == yaml_start {
                ""
            } else {
                &text[yaml_start..pos - 1]
            };
            let body = if end < text.len() {
                &text[end + 1..]
            } else {
                ""
            };
            return Ok(Some((yaml, body)));
        }
        if end == text.len() {
            return Err(FrontMatterError::Unterminated);
        }
        pos = end + 1;
    }
}

/// Parse the front matter of `text`. Returns `Ok(None)` if `text` does not start
/// with a `---` line.
pub fn parse(text: &str) -> Result<Option<FrontMatter>, FrontMatterError> {
    let Some((yaml_text, body)) = split(text)? else {
        return Ok(None);
    };
    let body = body.to_string();

    let map: std::collections::BTreeMap<String, serde_yaml_ng::Value> =
        if yaml_text.trim().is_empty() {
            std::collections::BTreeMap::new()
        } else {
            serde_yaml_ng::from_str(yaml_text)
                .map_err(|e| FrontMatterError::InvalidYaml(e.to_string()))?
        };

    let kind = map.get("type").map(value_to_string);
    let id = map.get("id").map(value_to_string);

    let title = map.get("title").map(value_to_string);
    let description = map.get("description").map(value_to_string);

    Ok(Some(FrontMatter {
        kind,
        id,
        title,
        description,
        body,
    }))
}

fn value_to_string(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Null => String::new(),
        other => {
            // Numbers, bools, etc. serialize back to their scalar text form.
            serde_yaml_ng::to_string(other)
                .unwrap_or_default()
                .trim()
                .to_string()
        }
    }
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

    // [FR-102]
    #[test]
    fn splitは閉じ無しや空など境界の入力を正しく分ける() {
        fn sp(t: &str) -> Result<Option<(&str, &str)>, ()> {
            split(t).map_err(|_| ())
        }
        assert_eq!(sp("---"), Err(()));
        assert_eq!(sp("---\n"), Err(()));
        assert_eq!(sp("---\na: 1\n"), Err(()));
        assert_eq!(sp("---\n---\n"), Ok(Some(("", ""))));
        assert_eq!(sp("---\na: 1\n---"), Ok(Some(("a: 1", ""))));
        assert_eq!(
            sp("---\r\na: 1\r\n---\r\nbody\r\n"),
            Ok(Some(("a: 1\r", "body\r\n")))
        );
        assert_eq!(
            sp("---\na: 1\n---\nx\n---\ny\n"),
            Ok(Some(("a: 1", "x\n---\ny\n")))
        );
        assert_eq!(sp("本文\n---\n"), Ok(None));
    }

    // [FR-102]
    #[test]
    fn parseのbodyはsplitの本文と一致する() {
        for text in [
            "---\n---\n",
            "---\na: 1\n---",
            "---\r\na: 1\r\n---\r\nbody\r\n",
            "---\na: 1\n---\nx\n---\ny\n",
        ] {
            let (_, body) = split(text).unwrap().unwrap();
            assert_eq!(parse(text).unwrap().unwrap().body, body, "{text:?}");
        }
    }
}
