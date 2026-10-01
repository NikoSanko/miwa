//! Test extraction for Go, Rust, C# and TypeScript source files.

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

const CSHARP_TEST_ATTRIBUTES: [&str; 7] = [
    "Fact",
    "Theory",
    "Test",
    "TestCase",
    "TestCaseSource",
    "TestMethod",
    "DataTestMethod",
];

/// Extract C# tests (xUnit / NUnit / MSTest) from a `*.cs` file: methods inside
/// types (also through namespaces and `#if` blocks) carrying a test attribute.
/// Markers come from the comment block immediately preceding the method, and
/// `content` runs from the first non-attribute token to the end of the method.
pub fn extract_csharp_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("valid c# grammar");
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
    let base = path.strip_suffix(".cs").unwrap_or(path);
    let mut results = Vec::new();
    walk_csharp_container(root, source, base, &[], &mut results);
    Ok(results)
}

fn walk_csharp_container(
    container: tree_sitter::Node,
    source: &str,
    base: &str,
    types: &[String],
    results: &mut Vec<ExtractedTest>,
) {
    let mut cursor = container.walk();
    let children: Vec<tree_sitter::Node> = container.children(&mut cursor).collect();
    for (idx, node) in children.iter().enumerate() {
        match node.kind() {
            "preproc_if" | "preproc_elif" | "preproc_else" => {
                walk_csharp_container(*node, source, base, types, results);
            }
            "namespace_declaration" => {
                if let Some(body) = node.child_by_field_name("body") {
                    walk_csharp_container(body, source, base, types, results);
                }
            }
            "class_declaration"
            | "struct_declaration"
            | "record_declaration"
            | "interface_declaration" => {
                let (Some(name_node), Some(body)) = (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("body"),
                ) else {
                    continue;
                };
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                let mut new_types = types.to_vec();
                new_types.push(name.to_string());
                walk_csharp_container(body, source, base, &new_types, results);
            }
            "method_declaration" if !types.is_empty() => {
                if !csharp_is_test_method(*node, source) {
                    continue;
                }
                let Some(name_node) = node.child_by_field_name("name") else {
                    continue;
                };
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                let mut c = node.walk();
                let Some(first) = node
                    .children(&mut c)
                    .find(|ch| ch.kind() != "attribute_list" && ch.kind() != "comment")
                else {
                    continue;
                };
                let content = source[first.start_byte()..node.end_byte()].to_string();
                let markers = collect_preceding_markers(&children, idx, source, "comment");
                results.push(ExtractedTest {
                    id: format!("{base}:{}:{name}", types.join("/")),
                    name,
                    markers,
                    content,
                });
            }
            _ => {}
        }
    }
}

fn csharp_is_test_method(method: tree_sitter::Node, source: &str) -> bool {
    let mut cursor = method.walk();
    for list in method.children(&mut cursor) {
        if list.kind() != "attribute_list" {
            continue;
        }
        let mut c2 = list.walk();
        for attr in list.children(&mut c2) {
            if attr.kind() != "attribute" {
                continue;
            }
            let Some(mut name) = attr.child_by_field_name("name") else {
                continue;
            };
            while name.kind() == "qualified_name" {
                match name.child_by_field_name("name") {
                    Some(n) => name = n,
                    None => break,
                }
            }
            let text = name.utf8_text(source.as_bytes()).unwrap_or("");
            if CSHARP_TEST_ATTRIBUTES.contains(&text) {
                return true;
            }
        }
    }
    false
}

const TYPESCRIPT_TEST_MODIFIERS: [&str; 6] =
    ["skip", "only", "todo", "fails", "concurrent", "sequential"];
const TYPESCRIPT_DESCRIBE_MODIFIERS: [&str; 6] = [
    "skip",
    "only",
    "todo",
    "concurrent",
    "sequential",
    "shuffle",
];

/// Extract TypeScript (Vitest) tests from a `*.test.ts` / `*.spec.ts` /
/// `*.test.tsx` / `*.spec.tsx` file: `it` / `test` call statements at the top
/// level or directly inside `describe` callbacks. Markers come from the
/// comment block immediately preceding the call, and `content` is the whole
/// call statement.
pub fn extract_typescript_tests(
    path: &str,
    source: &str,
) -> Result<Vec<ExtractedTest>, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = if path.ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    };
    parser
        .set_language(&lang)
        .expect("valid typescript grammar");
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
    let base = path
        .strip_suffix(".tsx")
        .or_else(|| path.strip_suffix(".ts"))
        .unwrap_or(path);
    let mut results = Vec::new();
    walk_typescript_statements(root, source, base, &[], &mut results);
    Ok(results)
}

/// Returns the base identifier and the property chain of a callee made only
/// of identifiers and member accesses (`test.concurrent.only`).
fn typescript_callee_chain<'a>(
    callee: tree_sitter::Node,
    source: &'a str,
) -> Option<(&'a str, Vec<&'a str>)> {
    let mut props = Vec::new();
    let mut cur = callee;
    loop {
        match cur.kind() {
            "identifier" => {
                props.reverse();
                return Some((cur.utf8_text(source.as_bytes()).ok()?, props));
            }
            "member_expression" => {
                let mut c = cur.walk();
                if cur.children(&mut c).any(|ch| ch.kind() == "optional_chain") {
                    return None;
                }
                let prop = cur.child_by_field_name("property")?;
                if prop.kind() != "property_identifier" {
                    return None;
                }
                props.push(prop.utf8_text(source.as_bytes()).ok()?);
                cur = cur.child_by_field_name("object")?;
            }
            _ => return None,
        }
    }
}

/// First-argument name: a plain string or a substitution-free template
/// string, sliced from the source without interpreting escapes.
fn typescript_name_arg<'a>(args: tree_sitter::Node, source: &'a str) -> Option<&'a str> {
    let first = args.named_child(0)?;
    match first.kind() {
        "string" => {}
        "template_string" => {
            let mut c = first.walk();
            if first
                .children(&mut c)
                .any(|ch| ch.kind() == "template_substitution")
            {
                return None;
            }
        }
        _ => return None,
    }
    let text = first.utf8_text(source.as_bytes()).ok()?;
    if text.len() < 2 {
        return None;
    }
    let name = &text[1..text.len() - 1];
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}'))
    {
        return None;
    }
    Some(name)
}

fn walk_typescript_statements(
    container: tree_sitter::Node,
    source: &str,
    base: &str,
    describes: &[String],
    results: &mut Vec<ExtractedTest>,
) {
    let mut cursor = container.walk();
    let children: Vec<tree_sitter::Node> = container.children(&mut cursor).collect();
    for (idx, node) in children.iter().enumerate() {
        if node.kind() != "expression_statement" {
            continue;
        }
        let Some(call) = node.named_child(0) else {
            continue;
        };
        if call.kind() != "call_expression" {
            continue;
        }
        let (Some(callee), Some(args)) = (
            call.child_by_field_name("function"),
            call.child_by_field_name("arguments"),
        ) else {
            continue;
        };
        let Some((head, props)) = typescript_callee_chain(callee, source) else {
            continue;
        };
        let Some(name) = typescript_name_arg(args, source) else {
            continue;
        };
        if (head == "test" || head == "it")
            && props.iter().all(|p| TYPESCRIPT_TEST_MODIFIERS.contains(p))
        {
            let markers = collect_preceding_markers(&children, idx, source, "comment");
            let content = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            results.push(ExtractedTest {
                id: format!("{base}:{}:{name}", describes.join("/")),
                name: name.to_string(),
                markers,
                content,
            });
        } else if head == "describe"
            && props
                .iter()
                .all(|p| TYPESCRIPT_DESCRIBE_MODIFIERS.contains(p))
        {
            let Some(cb) = args.named_child(1) else {
                continue;
            };
            if cb.kind() != "arrow_function" && cb.kind() != "function_expression" {
                continue;
            }
            let Some(body) = cb.child_by_field_name("body") else {
                continue;
            };
            if body.kind() != "statement_block" {
                continue;
            }
            let mut new_desc = describes.to_vec();
            new_desc.push(name.to_string());
            walk_typescript_statements(body, source, base, &new_desc, results);
        }
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

    // ---- C# ----

    fn cs(source: &str) -> Vec<ExtractedTest> {
        extract_csharp_tests("tests/AuthTests.cs", source).unwrap()
    }

    fn cs_wrap(method: &str) -> String {
        format!("public class AuthTests\n{{\n{method}\n}}\n")
    }

    // [FR-049]
    #[test]
    fn csharpはif_endif内のtestメソッドも検出する() {
        let source = "public class A {\n#if DEBUG\n    // [FR-001]\n    [Fact]\n    public void T() { }\n#endif\n}\n";
        let tests = extract_csharp_tests("tests/A.cs", source).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id, "tests/A:A:T");
        assert_eq!(tests[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-049]
    #[test]
    fn csharpは7種類の属性をそれぞれtestとして検出する() {
        for attr in [
            "Fact",
            "Theory",
            "Test",
            "TestCase",
            "TestCaseSource",
            "TestMethod",
            "DataTestMethod",
        ] {
            let source = cs_wrap(&format!("    [{attr}]\n    public void T() {{ }}"));
            let tests = cs(&source);
            assert_eq!(tests.len(), 1, "attribute {attr} should be detected");
            assert_eq!(tests[0].name, "T");
        }
    }

    // [FR-049]
    #[test]
    fn csharpはattribute接尾辞付きや無関係な属性のみのメソッドを検出しない() {
        let source = cs_wrap(
            "    [FactAttribute]\n    public void A() { }\n\n    [Obsolete]\n    public void B() { }\n\n    public void C() { }\n\n    [Category(\"x\")]\n    public void D() { }",
        );
        assert!(cs(&source).is_empty());
    }

    // [FR-049]
    #[test]
    fn csharpは引数付きの属性も検出する() {
        let source = cs_wrap("    [Fact(Skip = \"x\")]\n    public void T() { }");
        assert_eq!(cs(&source).len(), 1);
    }

    // [FR-049]
    #[test]
    fn csharpは名前空間修飾された属性も検出する() {
        let source = cs_wrap(
            "    [Xunit.Fact]\n    public void A() { }\n\n    [global::Xunit.Fact]\n    public void B() { }",
        );
        let tests = cs(&source);
        assert_eq!(tests.len(), 2);
        find(&tests, "A");
        find(&tests, "B");
    }

    // [FR-049]
    #[test]
    fn csharpはtarget指定付きの属性も検出する() {
        let source = cs_wrap("    [method: Fact]\n    public void T() { }");
        assert_eq!(cs(&source).len(), 1);
    }

    // [FR-049]
    #[test]
    fn csharpは同一属性リスト内の他の属性と並んだ属性も検出する() {
        let source = cs_wrap("    [Trait(\"a\", \"b\"), Fact]\n    public void T() { }");
        assert_eq!(cs(&source).len(), 1);
    }

    // [FR-049]
    #[test]
    fn csharpは複数の属性リストの順序に関わらず検出する() {
        let source = cs_wrap(
            "    [Theory]\n    [InlineData(1)]\n    public void A(int x) { }\n\n    [InlineData(1)]\n    [Theory]\n    public void B(int x) { }",
        );
        let tests = cs(&source);
        assert_eq!(tests.len(), 2);
        find(&tests, "A");
        find(&tests, "B");
    }

    // [FR-050]
    #[test]
    fn csharpは直前に隣接するコメントをマーカーとして抽出し空行を挟むと無視する() {
        let source = cs_wrap(
            "    // [FR-001]\n    [Fact]\n    public void Adjacent() { }\n\n    // [FR-002]\n\n    [Fact]\n    public void Blank() { }\n\n    [Fact]\n    public void None() { }",
        );
        let tests = cs(&source);
        assert_eq!(find(&tests, "Adjacent").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "Blank").markers.is_empty());
        assert!(find(&tests, "None").markers.is_empty());
    }

    // [FR-050]
    #[test]
    fn csharpはドキュメントコメントとブロックコメントもマーカーにできる() {
        let source = cs_wrap(
            "    /// [FR-001]\n    [Fact]\n    public void Doc() { }\n\n    /* [FR-002] */\n    [Fact]\n    public void Block() { }",
        );
        let tests = cs(&source);
        assert_eq!(find(&tests, "Doc").markers, vec!["FR-001".to_string()]);
        assert_eq!(find(&tests, "Block").markers, vec!["FR-002".to_string()]);
    }

    // [FR-050]
    #[test]
    fn csharpは属性と署名の間のコメントをマーカーにせずcontentにも含めない() {
        let source = cs_wrap(
            "    [Fact]\n    // [FR-001]\n    public void T() { }\n\n    [Fact]\n    [Trait(\"a\", \"b\")]\n    // 補足\n    public void U() { }",
        );
        let tests = cs(&source);
        let t = find(&tests, "T");
        assert!(t.markers.is_empty());
        assert!(!t.content.contains("FR-001"));
        let u = find(&tests, "U");
        assert!(!u.content.contains("補足"));
    }

    // [FR-050]
    #[test]
    fn csharpは属性リストの間のコメントをマーカーにせずcontentにも含めない() {
        let source = cs_wrap(
            "    [Theory]\n    // [FR-001]\n    [InlineData(1)]\n    public void T(int x) { }",
        );
        let tests = cs(&source);
        let t = find(&tests, "T");
        assert!(t.markers.is_empty());
        assert!(!t.content.contains("FR-001"));
    }

    // [FR-051]
    #[test]
    fn csharpのtest識別子はトップレベルclassならファイルパスと型名とメソッド名になる() {
        let source =
            "public class AuthTests\n{\n    [Fact]\n    public void RejectsShort() { }\n}\n";
        let tests = cs(source);
        assert_eq!(tests[0].id, "tests/AuthTests:AuthTests:RejectsShort");
    }

    // [FR-051]
    #[test]
    fn csharpのtest識別子はネストした型を外側から内側へスラッシュで連結する() {
        let source = "public class Outer\n{\n    public class Inner\n    {\n        [Test]\n        public void T3() { }\n    }\n}\n";
        let tests = cs(source);
        assert_eq!(tests[0].id, "tests/AuthTests:Outer/Inner:T3");
    }

    // [FR-051]
    #[test]
    fn csharpのtest識別子はstruct_record_interfaceの中でも型名を辿る() {
        let source = "public struct S\n{\n    [Fact]\n    public void A() { }\n}\n\npublic record R\n{\n    [Fact]\n    public void B() { }\n}\n\npublic interface I\n{\n    [Fact]\n    public void C() { }\n}\n";
        let tests = cs(source);
        assert_eq!(find(&tests, "A").id, "tests/AuthTests:S:A");
        assert_eq!(find(&tests, "B").id, "tests/AuthTests:R:B");
        assert_eq!(find(&tests, "C").id, "tests/AuthTests:I:C");
    }

    // [FR-051]
    #[test]
    fn csharpのtest識別子はblock_namespaceを含まない() {
        let source = "namespace Foo.Bar\n{\n    public class AuthTests\n    {\n        [Fact]\n        public void T() { }\n    }\n}\n";
        let tests = cs(source);
        assert_eq!(tests[0].id, "tests/AuthTests:AuthTests:T");
    }

    // [FR-051]
    #[test]
    fn csharpのtest識別子はfile_scoped_namespaceを含まない() {
        let source =
            "namespace Foo;\n\npublic class AuthTests\n{\n    [Fact]\n    public void T() { }\n}\n";
        let tests = cs(source);
        assert_eq!(tests[0].id, "tests/AuthTests:AuthTests:T");
    }

    // [FR-052]
    #[test]
    fn csharpのcontentは属性を含まず修飾子から始まる() {
        let source = cs_wrap(
            "    // [FR-001]\n    [Fact]\n    [Trait(\"a\", \"b\")]\n    public void T() { Assert.True(true); }",
        );
        let tests = cs(&source);
        let c = &tests[0].content;
        assert!(c.starts_with("public void T()"), "content = {c:?}");
        assert!(c.ends_with('}'));
        assert!(!c.contains("Fact"));
        assert!(!c.contains("Trait"));
        assert!(!c.contains("FR-001"));
    }

    // [FR-052]
    #[test]
    fn csharpの本体変更はcontentに反映される() {
        let a = cs(&cs_wrap(
            "    [Fact]\n    public void T() { Assert.True(true); }",
        ));
        let b = cs(&cs_wrap(
            "    [Fact]\n    public void T() { Assert.True(false); }",
        ));
        assert_ne!(a[0].content, b[0].content);
    }

    // [FR-052]
    #[test]
    fn csharpの属性だけの変更はcontentを変えない() {
        let a = cs(&cs_wrap(
            "    [Fact]\n    public void T() { Assert.True(true); }",
        ));
        let b = cs(&cs_wrap(
            "    [Fact(Skip = \"x\")]\n    [Trait(\"a\", \"b\")]\n    public void T() { Assert.True(true); }",
        ));
        assert_eq!(a[0].content, b[0].content);
    }

    // [FR-014]
    #[test]
    fn csharpは構文エラーのソースでエラーを返す() {
        let source = "public class A {\n    [Fact]\n    public void T( {\n}\n";
        assert!(extract_csharp_tests("tests/A.cs", source).is_err());
    }

    // ---- TypeScript ----

    fn ts(source: &str) -> Vec<ExtractedTest> {
        extract_typescript_tests("src/a.test.ts", source).unwrap()
    }

    // [FR-054]
    #[test]
    fn typescriptはtestとitの呼び出しを検出する() {
        let tests = ts("test(\"a\", () => {});\nit(\"b\", () => {});\n");
        assert_eq!(tests.len(), 2);
        find(&tests, "a");
        find(&tests, "b");
    }

    // [FR-054]
    #[test]
    fn typescriptは許可された修飾子付きの呼び出しを検出する() {
        let source = "it.skip(\"a\", () => {});\ntest.only(\"b\", () => {});\ntest.concurrent.only(\"c\", () => {});\ntest.todo(\"d\", () => {});\n";
        let tests = ts(source);
        assert_eq!(tests.len(), 4);
        for n in ["a", "b", "c", "d"] {
            find(&tests, n);
        }
    }

    // [FR-054]
    #[test]
    fn typescriptはdescribe_skipの中のtestも検出する() {
        let tests = ts("describe.skip(\"d\", () => {\n  it(\"a\", () => {});\n});\n");
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id, "src/a.test:d:a");
    }

    // [FR-054]
    #[test]
    fn typescriptはfunction式のコールバックも検出する() {
        let source = "describe(\"d\", function () {\n  it(\"a\", function () {});\n});\n";
        let tests = ts(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id, "src/a.test:d:a");
    }

    // [FR-053]
    #[test]
    fn typescriptはtsxファイルのjsxを含むソースも解析できる() {
        let source = "it(\"renders\", () => {\n  const x = <div className=\"a\">hi</div>;\n  expect(x).toBeTruthy();\n});\n";
        let tests = extract_typescript_tests("src/a.test.tsx", source).unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id, "src/a.test::renders");
    }

    // [FR-054]
    #[test]
    fn typescriptは許可外の修飾子や呼び出し結果の呼び出しを検出しない() {
        let source = r#"test.each([1, 2])("each %i", (n) => {});
test.skipIf(c)("skipif", () => {});
test.extend({ a: 1 });
test.describe("desc", () => {});
test.runIf(c)("runif", () => {});
it?.skip("optional", () => {});
"#;
        assert!(ts(source).is_empty());
    }

    // [FR-054]
    #[test]
    fn typescriptはexport_defaultやskipif_describeやforループ内のitを検出しない() {
        let source = r#"export default describe("d", () => { it("y", () => {}); })
describe.skipIf(c)("d", () => { it("x", () => {}); });
for (const n of [1]) {
  it("loop", () => {});
}
"#;
        assert!(ts(source).is_empty());
    }

    // [FR-054]
    #[test]
    fn typescriptは許可された修飾子を検出し走査する() {
        let source = r#"test.fails("f", () => {});
it.sequential("s", () => {});
describe.shuffle("d", () => {
  it("inner", () => {});
});
"#;
        let tests = ts(source);
        assert_eq!(tests.len(), 3);
        find(&tests, "f");
        find(&tests, "s");
        assert_eq!(find(&tests, "inner").id, "src/a.test:d:inner");
    }

    // [FR-054]
    #[test]
    fn typescriptはdescribe_eachの中のtestを検出しない() {
        let source = "describe.each([1])(\"d %i\", (n) => {\n  it(\"a\", () => {});\n});\n";
        assert!(ts(source).is_empty());
    }

    // [FR-054]
    #[test]
    fn typescriptはawaitなど式文直下でない呼び出しを検出しない() {
        let source = "await it(\"a\", () => {});\nvoid it(\"b\", () => {});\n";
        assert!(ts(source).is_empty());
    }

    // [FR-054]
    #[test]
    fn typescriptは第1引数が文字列リテラルでない呼び出しを検出しない() {
        let source = "const name = \"x\";\nit(name, () => {});\nit(\"a\" + \"b\", () => {});\n";
        assert!(ts(source).is_empty());
    }

    // [FR-054]
    #[test]
    fn typescriptはhelper関数やtest本体やif文の中のitを検出しない() {
        let source = r#"function helper() {
  it("in helper", () => {});
}
test("outer", () => {
  it("in test", () => {});
});
if (cond) {
  it("in if", () => {});
}
"#;
        let tests = ts(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "outer");
    }

    // [FR-056]
    #[test]
    fn typescriptは置換なしのテンプレート文字列を対象にし置換ありを無視する() {
        let source = "it(`plain`, () => {});\nit(`with ${x}`, () => {});\n";
        let tests = ts(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "plain");
    }

    // [FR-056]
    #[test]
    fn typescriptは空文字名と改行を含む名前を無視する() {
        let source = "it(\"\", () => {});\nit(`line1\nline2`, () => {});\nit(\"ok\", () => {});\n";
        let tests = ts(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "ok");
    }

    // [FR-056]
    #[test]
    fn typescriptは行区切り文字を含む名前を無視する() {
        let source = "it(\"a\u{2028}b\", () => {});\nit(\"ok\", () => {});\n";
        let tests = ts(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "ok");
    }

    // [FR-056]
    #[test]
    fn typescriptはエスケープを解釈せずソース表記のまま名前にする() {
        let source = "it('it\\'s', () => {});\n";
        let tests = ts(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "it\\'s");
        assert_eq!(tests[0].id, "src/a.test::it\\'s");
    }

    // [FR-056]
    #[test]
    fn typescriptのtest識別子はdescribeの外なら空のdescribeパスになる() {
        let tests = ts("it(\"top\", () => {});\n");
        assert_eq!(tests[0].id, "src/a.test::top");
    }

    // [FR-056]
    #[test]
    fn typescriptのtest識別子はdescribeを外側から内側へスラッシュで連結する() {
        let source = "describe(\"auth\", () => {\n  it(\"one\", () => {});\n  describe(\"inner\", () => {\n    it(\"two\", () => {});\n  });\n});\n";
        let tests = ts(source);
        assert_eq!(find(&tests, "one").id, "src/a.test:auth:one");
        assert_eq!(find(&tests, "two").id, "src/a.test:auth/inner:two");
    }

    // [FR-056]
    #[test]
    fn typescriptのtest識別子は拡張子のtsとtsxだけを除去する() {
        let a = extract_typescript_tests("src/a.spec.tsx", "it(\"t\", () => {});\n").unwrap();
        assert_eq!(a[0].id, "src/a.spec::t");
        let b = extract_typescript_tests("src/b.spec.ts", "it(\"t\", () => {});\n").unwrap();
        assert_eq!(b[0].id, "src/b.spec::t");
    }

    // [FR-055]
    #[test]
    fn typescriptは直前に隣接するコメントをマーカーとして抽出し空行を挟むと無視する() {
        let source = "// [FR-001]\nit(\"adjacent\", () => {});\n\n// [FR-002]\n\nit(\"blank\", () => {});\n\nit(\"none\", () => {});\n";
        let tests = ts(source);
        assert_eq!(find(&tests, "adjacent").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "blank").markers.is_empty());
        assert!(find(&tests, "none").markers.is_empty());
    }

    // [FR-055]
    #[test]
    fn typescriptはdescribe内のtestでも直前のコメントをマーカーにする() {
        let source = "describe(\"d\", () => {\n  // [FR-001]\n  it(\"a\", () => {});\n});\n";
        let tests = ts(source);
        assert_eq!(tests[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-003]
    #[test]
    fn typescriptのcontentは呼び出し文全体でマーカーコメントを含まない() {
        let source = "// [FR-001]\nit(\"a\", () => {\n  expect(1).toBe(1);\n});\n";
        let tests = ts(source);
        let c = &tests[0].content;
        assert!(c.starts_with("it(\"a\""), "content = {c:?}");
        assert!(c.ends_with(");"), "content = {c:?}");
        assert!(!c.contains("FR-001"));
    }

    // [FR-003]
    #[test]
    fn typescriptの本体変更はcontentに反映される() {
        let a = ts("it(\"a\", () => { expect(1).toBe(1); });\n");
        let b = ts("it(\"a\", () => { expect(1).toBe(2); });\n");
        assert_ne!(a[0].content, b[0].content);
    }

    // [FR-014]
    #[test]
    fn typescriptは構文エラーのソースでエラーを返す() {
        let source = "it(\"a\", () => {\n";
        assert!(extract_typescript_tests("src/a.test.ts", source).is_err());
    }
}
