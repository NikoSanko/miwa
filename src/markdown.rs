//! Markdown link extraction and resolution.

/// Extract inline and reference-style link targets from `body`, in order of
/// appearance. Image links are excluded. Fragments are kept as-is.
pub fn extract_links(body: &str) -> Vec<String> {
    let _ = body;
    todo!()
}

/// Resolve `target` (a link found in `doc_path`, a repo-relative `/`-separated
/// path) into a normalized repo-relative path. Returns `None` for external
/// URLs, absolute paths, anchor-only targets, or targets that escape the repo.
/// The fragment (if any) is stripped from the result.
pub fn resolve_link(doc_path: &str, target: &str) -> Option<String> {
    let _ = (doc_path, target);
    todo!()
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
}
