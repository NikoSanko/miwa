//! Markdown link extraction and resolution.

/// Extract inline and reference-style link targets from `body`, in order of
/// appearance. Image links are excluded. Fragments are kept as-is.
pub fn extract_links(body: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag};

    let parser = Parser::new(body);
    let mut links = Vec::new();
    for event in parser {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            links.push(dest_url.to_string());
        }
    }
    links
}

/// Resolve `target` (a link found in `doc_path`, a repo-relative `/`-separated
/// path) into a normalized repo-relative path. Returns `None` for external
/// URLs, absolute paths, anchor-only targets, or targets that escape the repo.
/// The fragment (if any) is stripped from the result.
pub fn resolve_link(doc_path: &str, target: &str) -> Option<String> {
    if target.is_empty()
        || target.contains("://")
        || target.starts_with('/')
        || target.starts_with('#')
        || has_uri_scheme(target)
    {
        return None;
    }

    // Strip the fragment, if any.
    let target = match target.split_once('#') {
        Some((before, _)) => before,
        None => target,
    };
    if target.is_empty() {
        return None;
    }

    let parent: Vec<&str> = {
        let mut segs: Vec<&str> = doc_path.split('/').collect();
        segs.pop(); // drop the file name
        segs
    };

    let mut segments: Vec<&str> = parent;
    for part in target.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }

    if segments.is_empty() {
        return None;
    }

    Some(segments.join("/"))
}

/// Whether `s` starts with a URI scheme (`^[A-Za-z][A-Za-z0-9+.-]*:`), e.g.
/// `mailto:`, `data:`, `https:`. Windows drive letters (`C:\...`) are not
/// treated specially.
fn has_uri_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-') {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // [FR-008]
    #[test]
    fn インライン形式のリンクをdoc_reqエッジとして抽出できる() {
        let body = "パスワードは [最小長](../req/auth/password-min-length.md) に従う。\n";
        let links = extract_links(body);
        assert_eq!(
            links,
            vec!["../req/auth/password-min-length.md".to_string()]
        );
        let resolved = resolve_link("docs/model.md", &links[0]).unwrap();
        assert_eq!(resolved, "req/auth/password-min-length.md");
    }

    // [FR-008]
    #[test]
    fn 参照形式のリンクをdoc_reqエッジとして抽出できる() {
        let body =
            "パスワードは [最小長][ref] に従う。\n\n[ref]: ../req/auth/password-min-length.md\n";
        let links = extract_links(body);
        assert_eq!(
            links,
            vec!["../req/auth/password-min-length.md".to_string()]
        );
        let resolved = resolve_link("docs/model.md", &links[0]).unwrap();
        assert_eq!(resolved, "req/auth/password-min-length.md");
    }

    // [FR-009]
    #[test]
    fn resolve_linkはフラグメントを取り除いてから解決する() {
        let resolved = resolve_link("docs/model.md", "../req/auth/password-min-length.md#見出し");
        assert_eq!(
            resolved,
            Some("req/auth/password-min-length.md".to_string())
        );
    }

    // [FR-010]
    #[test]
    fn 画像リンクはextract_linksの対象にならない() {
        let body = "説明図です。\n\n![alt](../req/diagram.png)\n";
        let links = extract_links(body);
        assert!(links.is_empty(), "image link should be excluded: {links:?}");
    }

    // [FR-010]
    #[test]
    fn コードブロックやインラインコード内のリンクは対象にならない() {
        let body = "インラインコード: `[text](../req/x.md)`\n\n```\n[text](../req/y.md)\n```\n";
        let links = extract_links(body);
        assert!(
            links.is_empty(),
            "links inside code should be excluded: {links:?}"
        );
    }

    // [FR-010]
    #[test]
    fn 外部urlやリポジトリ外へ逃げるパスはresolve_linkでnoneになる() {
        assert_eq!(
            resolve_link("docs/model.md", "https://example.com/req/x.md"),
            None
        );
        assert_eq!(
            resolve_link("docs/model.md", "http://example.com/req/x.md"),
            None
        );
        assert_eq!(resolve_link("docs/model.md", "../../../etc/passwd"), None);
        assert_eq!(resolve_link("docs/model.md", "#見出しだけ"), None);
    }

    // [FR-010]
    #[test]
    fn mailtoやdataスキームのリンクはresolve_linkでnoneになる() {
        assert_eq!(
            resolve_link("docs/model.md", "mailto:someone@example.com"),
            None
        );
        assert_eq!(
            resolve_link("docs/model.md", "data:text/plain;base64,aGVsbG8="),
            None
        );
    }
}
