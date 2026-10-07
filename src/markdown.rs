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

    normalize_relative(doc_path, target)
}

/// Join `target` (a `/`-separated relative path, no fragment) onto the parent
/// directory of `base_path` and normalize `.` / `..` / empty segments. Returns
/// `None` if the result escapes the repository or is empty.
pub(crate) fn normalize_relative(base_path: &str, target: &str) -> Option<String> {
    let mut segments: Vec<&str> = {
        let mut segs: Vec<&str> = base_path.split('/').collect();
        segs.pop(); // drop the file name
        segs
    };
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

/// Decode `%XX` escapes as UTF-8 bytes. Invalid escapes (and results that are
/// not valid UTF-8) are left as-is.
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Whether `s` starts with a URI scheme (`^[A-Za-z][A-Za-z0-9+.-]*:`), e.g.
/// `mailto:`, `data:`, `https:`. Windows drive letters (`C:\...`) are not
/// treated specially.
pub(crate) fn has_uri_scheme(s: &str) -> bool {
    uri_scheme(s).is_some()
}

/// The URI scheme of `s` (without the colon), if it starts with one.
pub(crate) fn uri_scheme(s: &str) -> Option<&str> {
    let mut chars = s.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() => {}
        _ => return None,
    }
    for (i, c) in chars {
        if c == ':' {
            return Some(&s[..i]);
        }
        if !(c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-') {
            return None;
        }
    }
    None
}

/// Directory part of a `/`-separated path (`""` when there is none).
pub(crate) fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
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

    // [FR-070]
    #[test]
    fn percent_decodeは通常の列をutf8としてデコードする() {
        assert_eq!(percent_decode("%E9%9B%86.md"), "集.md");
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("a%23b"), "a#b");
        assert_eq!(percent_decode("x%41"), "xA");
        assert_eq!(percent_decode("plain"), "plain");
    }

    // [FR-070]
    #[test]
    fn percent_decodeは不正な列をそのまま残す() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("%E9"), "%E9");
    }

    // [FR-070]
    #[test]
    fn normalize_relativeはresolve_linkと同じ正規化をする() {
        let cases = [
            ("docs/index.md", "a.md"),
            ("docs/index.md", "./a/../b.md"),
            ("docs/sub/index.md", "../a.md"),
            ("docs/index.md", "sub//x.md"),
            ("docs/index.md", "../../out.md"),
            ("docs/index.md", "../.."),
        ];
        for (base, target) in cases {
            assert_eq!(
                normalize_relative(base, target),
                resolve_link(base, target),
                "{base} {target}"
            );
        }
        assert_eq!(
            normalize_relative("docs/sub/index.md", "../a.md"),
            Some("docs/a.md".to_string())
        );
        assert_eq!(normalize_relative("docs/index.md", "../../out.md"), None);
    }
}
