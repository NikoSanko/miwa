//! `miwa site`: static HTML output for reading the documents.
//!
//! Skeleton only (RED phase): bodies are `todo!()`.
#![allow(unused_variables)]

use crate::config::Config;
use std::path::Path;

/// A relative link destination resolved against the source file's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLink {
    /// Repository-relative path (normalized, percent-decoded).
    pub path: String,
    /// Fragment after `#`, if any.
    pub fragment: Option<String>,
}

/// Writes the site into `out` and returns warning lines (without trailing newline).
pub fn run(repo_root: &Path, config: &Config, out: &Path) -> anyhow::Result<Vec<String>> {
    todo!()
}

/// GitHub-compatible heading id for the rendered plain text of a heading
/// (empty when nothing remains).
pub fn heading_slug(text: &str) -> String {
    todo!()
}

/// Resolves a relative link / image destination from a file in `source_dir`
/// (repository-relative directory, `""` for the root). `None` when the
/// destination is empty (after dropping `?...`) or escapes the repository.
pub fn resolve_link_path(source_dir: &str, dest: &str) -> Option<ResolvedLink> {
    todo!()
}

/// Builds an href from a resolved path: each segment percent-encoded except
/// RFC 3986 pchar, a `:` in the first segment encoded as `%3A`, fragment
/// appended after `#`.
pub fn encode_href(path: &str, fragment: Option<&str>) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(source_dir: &str, dest: &str) -> Option<(String, Option<String>)> {
        resolve_link_path(source_dir, dest).map(|r| (r.path, r.fragment))
    }

    // [FR-106]
    #[test]
    fn 見出しidは小文字化し記号を除き空白をハイフンにする() {
        assert_eq!(heading_slug("Hello, World!"), "hello-world");
        assert_eq!(heading_slug("A_b-c 1"), "a_b-c-1");
    }

    // [FR-106]
    #[test]
    fn 見出しidは連続する空白を1つずつハイフンにする() {
        assert_eq!(heading_slug("a  b"), "a--b");
    }

    // [FR-106]
    #[test]
    fn 見出しidはunicodeの文字と数字を残し小文字にする() {
        assert_eq!(heading_slug("ÄÖ Straße"), "äö-straße");
        assert_eq!(heading_slug("日本語 見出し２"), "日本語-見出し２");
    }

    // [FR-106]
    #[test]
    fn 見出しidはタブと全角空白を除去する() {
        assert_eq!(heading_slug("a\tb"), "ab");
        assert_eq!(heading_slug("a\u{3000}b"), "ab");
        assert_eq!(heading_slug("a b"), "a-b");
    }

    // [FR-106]
    #[test]
    fn 見出しidは記号だけなら空になる() {
        assert_eq!(heading_slug("!!!"), "");
        assert_eq!(heading_slug("***"), "");
    }

    // [FR-107]
    #[test]
    fn リンク解決は元ファイルのディレクトリ基準でドットを正規化する() {
        assert_eq!(
            resolved("docs/a", "../req/x.md"),
            Some(("docs/req/x.md".to_string(), None))
        );
        assert_eq!(
            resolved("docs", "./sub/./y.md"),
            Some(("docs/sub/y.md".to_string(), None))
        );
        assert_eq!(resolved("", "x.md"), Some(("x.md".to_string(), None)));
    }

    // [FR-107]
    #[test]
    fn リンク解決はクエリを捨ててフラグメントを保持する() {
        assert_eq!(
            resolved("docs", "x.md?v=1#top"),
            Some(("docs/x.md".to_string(), Some("top".to_string())))
        );
        assert_eq!(
            resolved("docs", "x.md#top"),
            Some(("docs/x.md".to_string(), Some("top".to_string())))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決はパーセントデコードしデコード後のシャープを再分割しない() {
        assert_eq!(
            resolved("docs", "a%20b.md"),
            Some(("docs/a b.md".to_string(), None))
        );
        assert_eq!(
            resolved("docs", "a%23b.md#f"),
            Some(("docs/a#b.md".to_string(), Some("f".to_string())))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決はクエリだけや空やリポジトリ外をnoneにする() {
        assert_eq!(resolved("docs", ""), None);
        assert_eq!(resolved("docs", "?q=1"), None);
        assert_eq!(resolved("docs", "../../x.md"), None);
    }

    // [FR-107]
    #[test]
    fn リンク解決はクエリを先に捨ててからデコードする() {
        assert_eq!(
            resolved("docs", "a%3Fb.md"),
            Some(("docs/a?b.md".to_string(), None))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決はフラグメント中のクエスチョンを保持する() {
        assert_eq!(
            resolved("docs", "x.md#a?b"),
            Some(("docs/x.md".to_string(), Some("a?b".to_string())))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決のドットは自ディレクトリを指す() {
        assert_eq!(
            resolved("docs/sub", "."),
            Some(("docs/sub".to_string(), None))
        );
    }

    // [FR-107]
    #[test]
    fn hrefはpchar以外をセグメントごとにパーセントエンコードする() {
        assert_eq!(encode_href("docs/a b.html", None), "docs/a%20b.html");
        assert_eq!(
            encode_href("docs/日本.html", None),
            "docs/%E6%97%A5%E6%9C%AC.html"
        );
        assert_eq!(encode_href("docs/a#b.html", None), "docs/a%23b.html");
        assert_eq!(encode_href("docs/a?b.html", None), "docs/a%3Fb.html");
        assert_eq!(encode_href("docs/a@b+c.html", None), "docs/a@b+c.html");
        assert_eq!(encode_href("docs/100%.html", None), "docs/100%25.html");
    }

    // [FR-107]
    #[test]
    fn hrefの最初のセグメントのコロンだけを3aにする() {
        assert_eq!(encode_href("a:b/c.html", None), "a%3Ab/c.html");
        assert_eq!(encode_href("docs/a:b.html", None), "docs/a:b.html");
    }

    // [FR-107]
    #[test]
    fn hrefはフラグメントの有効なパーセント表記を保持し他のパーセントを符号化する() {
        assert_eq!(encode_href("x.html", Some("%E6%A6%82")), "x.html#%E6%A6%82");
        assert_eq!(encode_href("x.html", Some("100%")), "x.html#100%25");
    }

    // [FR-107]
    #[test]
    fn hrefはフラグメントをシャープで付ける() {
        assert_eq!(encode_href("x.html", Some("sec")), "x.html#sec");
    }
}
