//! Text normalization and content hashing.

/// Normalize line endings (CRLF / CR) to LF. Nothing else is changed.
pub fn normalize(text: &str) -> String {
    let _ = text;
    todo!()
}

/// Normalize `text` and return its SHA-256 hash as `"sha256:<hex>"` (lowercase hex).
pub fn content_hash(text: &str) -> String {
    let _ = text;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // [FR-001]
    #[test]
    fn crlf_と_lf_は同じハッシュになる() {
        let lf = "line one\nline two\n";
        let crlf = "line one\r\nline two\r\n";
        assert_eq!(content_hash(lf), content_hash(crlf));
    }

    // [FR-001]
    #[test]
    fn content_hash_はsha256プレフィックス付きの小文字hexを返す() {
        let hash = content_hash("hello");
        assert!(hash.starts_with("sha256:"), "unexpected hash: {hash}");
        let hex = hash.strip_prefix("sha256:").unwrap();
        assert_eq!(hex.len(), 64);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // 空白除去や大小文字変換のような正規化は行われないため、内容が違えば別ハッシュになる
        assert_ne!(content_hash("hello"), content_hash("hello "));
    }
}
