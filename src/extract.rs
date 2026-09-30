//! Test extraction for Go and Rust source files.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedTest {
    pub id: String,
    pub name: String,
    pub markers: Vec<String>,
    pub content: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("parse error in {path}")]
    Parse { path: String },
}

/// Extract Go tests from a `*_test.go` file: top-level `function_declaration`s
/// whose name starts with `Test`. Markers are collected from the block of
/// comments immediately (no blank line) preceding the function.
pub fn extract_go_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    let _ = (path, source);
    todo!()
}

/// Extract Rust tests: `function_item`s carrying a literal `#[test]`
/// attribute. Markers come from the comment block immediately preceding
/// `#[test]`. `content` is the source of the `function_item` (attributes not
/// included).
pub fn extract_rust_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    let _ = (path, source);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(tests: &'a [ExtractedTest], name: &str) -> &'a ExtractedTest {
        tests
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("test named {name} not found in {tests:?}"))
    }

    // [FR-012]
    #[test]
    fn goは直前に隣接するコメントをマーカーとして抽出する() {
        let source = r#"package auth

// [FR-001]
func TestRejectsShortPassword(t *testing.T) {
    doSomething()
}
"#;
        let tests = extract_go_tests("internal/auth/login_test.go", source).unwrap();
        let t = find(&tests, "TestRejectsShortPassword");
        assert_eq!(t.markers, vec!["FR-001".to_string()]);
    }

    // [FR-012]
    #[test]
    fn goは空行を挟んだコメントや欠落をマーカーなしとして無視する() {
        let source = r#"package auth

// [FR-002]

func TestWithBlankLine(t *testing.T) {
}

func TestNoComment(t *testing.T) {
}
"#;
        let tests = extract_go_tests("internal/auth/login_test.go", source).unwrap();
        assert!(find(&tests, "TestWithBlankLine").markers.is_empty());
        assert!(find(&tests, "TestNoComment").markers.is_empty());
    }

    // [FR-013]
    #[test]
    fn goのtest識別子はディレクトリと関数名を組み合わせた形式になる() {
        let source = "package auth\n\n// [FR-001]\nfunc TestLogin(t *testing.T) {}\n";
        let tests = extract_go_tests("internal/auth/login_test.go", source).unwrap();
        assert_eq!(find(&tests, "TestLogin").id, "internal/auth:TestLogin");
    }

    // [FR-013]
    #[test]
    fn goのtest識別子はリポジトリ直下ならドットになる() {
        let source = "package main\n\n// [FR-001]\nfunc TestRoot(t *testing.T) {}\n";
        let tests = extract_go_tests("login_test.go", source).unwrap();
        assert_eq!(find(&tests, "TestRoot").id, ".:TestRoot");
    }

    // [FR-003]
    #[test]
    fn rustの関数本体の変更はcontentに反映される() {
        let base = r#"// [FR-001]
#[test]
fn rejects_short_password() {
    assert!(true);
}
"#;
        let changed = r#"// [FR-001]
#[test]
fn rejects_short_password() {
    assert!(false);
}
"#;
        let a = find(
            &extract_rust_tests("src/auth.rs", base).unwrap(),
            "rejects_short_password",
        )
        .content
        .clone();
        let b = find(
            &extract_rust_tests("src/auth.rs", changed).unwrap(),
            "rejects_short_password",
        )
        .content
        .clone();
        assert_ne!(a, b);
        assert!(a.contains("fn rejects_short_password"));
        assert!(a.contains("assert!(true)"));
    }

    // [FR-003]
    #[test]
    fn goの関数本体の変更はcontentに反映される() {
        let base = "package auth\n\n// [FR-001]\nfunc TestRejectsShortPassword(t *testing.T) {\n\tassertTrue(t, true)\n}\n";
        let changed = "package auth\n\n// [FR-001]\nfunc TestRejectsShortPassword(t *testing.T) {\n\tassertTrue(t, false)\n}\n";
        let a = find(
            &extract_go_tests("internal/auth/login_test.go", base).unwrap(),
            "TestRejectsShortPassword",
        )
        .content
        .clone();
        let b = find(
            &extract_go_tests("internal/auth/login_test.go", changed).unwrap(),
            "TestRejectsShortPassword",
        )
        .content
        .clone();
        assert_ne!(a, b);
        assert!(a.contains("func TestRejectsShortPassword"));
        assert!(a.contains("assertTrue(t, true)"));
    }

    // [FR-004]
    #[test]
    fn rustのcontentは外側の属性とマーカーコメントを含まない() {
        let source = r#"// [FR-001]
#[test]
fn rejects_short_password() {
    assert!(true);
}
"#;
        let extracted = extract_rust_tests("src/auth.rs", source).unwrap();
        let t = find(&extracted, "rejects_short_password");
        assert!(!t.content.contains("#[test]"));
        assert!(!t.content.contains("[FR-001]"));
        assert!(t.content.contains("fn rejects_short_password"));
    }

    // [FR-015]
    #[test]
    fn リテラルなtest属性のみをrustのtestとして抽出する() {
        let source = r#"// [FR-001]
#[test]
fn plain_test() {}

// [FR-002]
#[tokio::test]
async fn async_test() {}
"#;
        let tests = extract_rust_tests("src/auth.rs", source).unwrap();
        assert!(tests.iter().any(|t| t.name == "plain_test"));
        assert!(!tests.iter().any(|t| t.name == "async_test"));
    }

    // [FR-016]
    #[test]
    fn rustはtest属性の直前に隣接するコメントをマーカーとする() {
        let source = r#"// [FR-001]
#[test]
fn adjacent() {}

// [FR-002]

#[test]
fn blank_line_between() {}

#[test]
fn no_comment() {}
"#;
        let tests = extract_rust_tests("src/auth.rs", source).unwrap();
        assert_eq!(find(&tests, "adjacent").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "blank_line_between").markers.is_empty());
        assert!(find(&tests, "no_comment").markers.is_empty());
    }

    // [FR-017]
    #[test]
    fn rustのtest識別子はトップレベルなら空モジュールパスになる() {
        let source = "// [FR-001]\n#[test]\nfn roundtrip() {}\n";
        let tests = extract_rust_tests("src/lock.rs", source).unwrap();
        assert_eq!(find(&tests, "roundtrip").id, "src/lock::roundtrip");
    }

    // [FR-017]
    #[test]
    fn rustのtest識別子はinline_modを外側から内側の順に連結する() {
        let source = r#"mod tests {
    // [FR-001]
    #[test]
    fn roundtrip() {}
}
"#;
        let tests = extract_rust_tests("src/lock.rs", source).unwrap();
        assert_eq!(find(&tests, "roundtrip").id, "src/lock:tests:roundtrip");
    }
}
