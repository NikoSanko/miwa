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
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("valid go grammar");
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ExtractError::Parse {
            path: path.to_string(),
        })?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(ExtractError::Parse {
            path: path.to_string(),
        });
    }

    let dir = dir_of(path);
    let mut results = Vec::new();

    let mut cursor = root.walk();
    let children: Vec<tree_sitter::Node> = root.children(&mut cursor).collect();
    for (idx, node) in children.iter().enumerate() {
        if node.kind() != "function_declaration" {
            continue;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            continue;
        };
        let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
        if !name.starts_with("Test") {
            continue;
        }
        let markers = collect_preceding_markers(&children, idx, source, "comment");
        let content = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        results.push(ExtractedTest {
            id: format!("{dir}:{name}"),
            name: name.to_string(),
            markers,
            content,
        });
    }
    Ok(results)
}

/// Extract Rust tests: `function_item`s carrying a literal `#[test]`
/// attribute. Markers come from the comment block immediately preceding
/// `#[test]`. `content` is the source of the `function_item` (attributes not
/// included).
pub fn extract_rust_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("valid rust grammar");
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ExtractError::Parse {
            path: path.to_string(),
        })?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(ExtractError::Parse {
            path: path.to_string(),
        });
    }

    let base = path.strip_suffix(".rs").unwrap_or(path);
    let mut results = Vec::new();
    walk_rust_items(root, source, base, &[], &mut results);
    Ok(results)
}

fn walk_rust_items(
    container: tree_sitter::Node,
    source: &str,
    base: &str,
    mods: &[String],
    results: &mut Vec<ExtractedTest>,
) {
    let mut cursor = container.walk();
    let children: Vec<tree_sitter::Node> = container.children(&mut cursor).collect();
    for (idx, node) in children.iter().enumerate() {
        match node.kind() {
            "function_item" => {
                let Some(name_node) = node.child_by_field_name("name") else {
                    continue;
                };
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                let Some(markers) = rust_test_markers(&children, idx, source) else {
                    continue;
                };
                let content = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                let id = if mods.is_empty() {
                    format!("{base}::{name}")
                } else {
                    format!("{base}:{}:{name}", mods.join("/"))
                };
                results.push(ExtractedTest {
                    id,
                    name,
                    markers,
                    content,
                });
            }
            "mod_item" => {
                let Some(name_node) = node.child_by_field_name("name") else {
                    continue;
                };
                let Some(body) = node.child_by_field_name("body") else {
                    continue;
                };
                let mod_name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                let mut new_mods = mods.to_vec();
                new_mods.push(mod_name);
                walk_rust_items(body, source, base, &new_mods, results);
            }
            _ => {}
        }
    }
}

/// Walks backward from `children[target_idx]`, collecting a contiguous
/// (no-blank-line) block of `comment_kind` nodes, and extracts `[marker]`
/// tokens from them, top to bottom.
fn collect_preceding_markers(
    children: &[tree_sitter::Node],
    target_idx: usize,
    source: &str,
    comment_kind: &str,
) -> Vec<String> {
    let mut next_start_row = children[target_idx].start_position().row;
    let mut j = target_idx as isize - 1;
    let mut comments = Vec::new();
    while j >= 0 {
        let node = children[j as usize];
        if node.kind() != comment_kind {
            break;
        }
        let end_row = node.end_position().row;
        if end_row + 1 != next_start_row {
            break;
        }
        comments.push(node);
        next_start_row = node.start_position().row;
        j -= 1;
    }
    comments.reverse();
    let mut markers = Vec::new();
    for c in comments {
        let text = c.utf8_text(source.as_bytes()).unwrap_or("");
        markers.extend(extract_markers(text));
    }
    markers
}

/// Returns `None` if `children[func_idx]` is not preceded (immediately) by an
/// `attribute_item` chain containing a literal `#[test]`. Otherwise, returns
/// the markers from the comment block immediately (no blank line, no other
/// attribute in between) preceding that specific `#[test]` attribute node
/// (not just the start of the whole attribute chain).
fn rust_test_markers(
    children: &[tree_sitter::Node],
    func_idx: usize,
    source: &str,
) -> Option<Vec<String>> {
    // Comments may sit between the attribute chain and the function itself
    // (e.g. `#[test]` / `// note` / `fn f() {}`). Skip over those before
    // looking for the attribute chain; they never contribute markers (a
    // marker must be immediately before `#[test]` itself, handled below).
    let mut j = func_idx as isize - 1;
    while j >= 0 && is_comment_kind(children[j as usize].kind()) {
        j -= 1;
    }

    let mut test_attr_idx: Option<usize> = None;
    while j >= 0 && children[j as usize].kind() == "attribute_item" {
        let attr = children[j as usize];
        let text = attr.utf8_text(source.as_bytes()).unwrap_or("").trim();
        if text == "#[test]" {
            test_attr_idx = Some(j as usize);
        }
        j -= 1;
    }
    let test_attr_idx = test_attr_idx?;

    let mut next_start_row = children[test_attr_idx].start_position().row;
    let mut k = test_attr_idx as isize - 1;
    let mut comments = Vec::new();
    while k >= 0 {
        let node = children[k as usize];
        if !is_comment_kind(node.kind()) {
            break;
        }
        let end_row = node.end_position().row;
        if end_row + 1 != next_start_row {
            break;
        }
        comments.push(node);
        next_start_row = node.start_position().row;
        k -= 1;
    }
    comments.reverse();
    let mut markers = Vec::new();
    for c in comments {
        let text = c.utf8_text(source.as_bytes()).unwrap_or("");
        markers.extend(extract_markers(text));
    }
    Some(markers)
}

fn is_comment_kind(kind: &str) -> bool {
    kind == "line_comment" || kind == "block_comment"
}

/// Extracts every `[marker]` occurrence, where `marker` is one or more
/// `[A-Za-z0-9._-]` characters.
fn extract_markers(text: &str) -> Vec<String> {
    let mut markers = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after_open = &rest[start + 1..];
        let Some(end) = after_open.find(']') else {
            break;
        };
        let inner = &after_open[..end];
        if !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            markers.push(inner.to_string());
        }
        rest = &after_open[end + 1..];
    }
    markers
}

fn dir_of(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => ".".to_string(),
    }
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

    // [FR-017]
    #[test]
    fn rustのtest識別子はネストしたinline_modをスラッシュで連結する() {
        let source = r#"mod outer {
    mod inner {
        // [FR-001]
        #[test]
        fn t() {}
    }
}
"#;
        let tests = extract_rust_tests("src/x.rs", source).unwrap();
        assert_eq!(find(&tests, "t").id, "src/x:outer/inner:t");
    }

    // [FR-016]
    #[test]
    fn rustは属性が間に挟まるとtestの直前のコメントをマーカーにしない() {
        let source = r#"// [FR-001]
#[cfg(feature = "x")]
#[test]
fn not_adjacent() {}
"#;
        let tests = extract_rust_tests("src/auth.rs", source).unwrap();
        assert!(find(&tests, "not_adjacent").markers.is_empty());
    }

    // [FR-016]
    #[test]
    fn rustは他の属性の後ろでもtestの直前のコメントはマーカーになる() {
        let source = r#"#[cfg(feature = "x")]
// [FR-001]
#[test]
fn adjacent_after_other_attr() {}
"#;
        let tests = extract_rust_tests("src/auth.rs", source).unwrap();
        assert_eq!(
            find(&tests, "adjacent_after_other_attr").markers,
            vec!["FR-001".to_string()]
        );
    }

    // [FR-015]
    #[test]
    fn rustはtestとfnの間にコメントがあってもtestとして検出しマーカーを採用する() {
        let source = r#"// [FR-001]
#[test]
// 補足
fn t() {}
"#;
        let tests = extract_rust_tests("src/auth.rs", source).unwrap();
        let found = find(&tests, "t");
        assert_eq!(found.markers, vec!["FR-001".to_string()]);
        assert!(!found.content.contains("補足"));
    }
}
