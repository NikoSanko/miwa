//! Test extraction for Go, Rust, C#, Java, Kotlin, TypeScript, JavaScript and Dart source files.

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

/// Extract Rust tests: `function_item`s carrying a test attribute, i.e. an
/// outer attribute whose path ends in `test` (`#[test]`, `#[tokio::test]`, ...)
/// or in one of `extra_attributes` (last path segment, e.g. `rstest`).
/// Markers come from the comment block immediately preceding the first
/// (topmost) test attribute. `content` is the source of the `function_item`
/// (attributes not included).
pub fn extract_rust_tests(
    path: &str,
    source: &str,
    extra_attributes: &[String],
) -> Result<Vec<ExtractedTest>, ExtractError> {
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
    walk_rust_items(root, source, base, &[], extra_attributes, &mut results);
    Ok(results)
}

fn walk_rust_items(
    container: tree_sitter::Node,
    source: &str,
    base: &str,
    mods: &[String],
    extra_attributes: &[String],
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
                let Some(markers) = rust_test_markers(&children, idx, source, extra_attributes)
                else {
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
                walk_rust_items(body, source, base, &new_mods, extra_attributes, results);
            }
            _ => {}
        }
    }
}

/// Last row a comment occupies. tree-sitter-rust includes the trailing newline
/// of `///` / `//!` line comments, so such a node ends at column 0 of the
/// next row; that row is not part of the comment.
fn comment_end_row(node: tree_sitter::Node) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

/// Whether only whitespace (or a BOM) precedes the node on its line in
/// `parsed` (the string the tree was built from). FR-064.
fn is_own_line_comment(parsed: &str, node: tree_sitter::Node) -> bool {
    let s = node.start_byte();
    let Some(head) = parsed.get(..s) else {
        return false;
    };
    let ls = head.rfind('\n').map_or(0, |i| i + 1);
    head[ls..]
        .chars()
        .all(|c| c.is_whitespace() || c == '\u{feff}')
}

/// Walks backward from `children[target_idx]`, collecting a contiguous
/// (no-blank-line) block of own-line comment nodes whose kind is in
/// `comment_kinds`, and extracts `[marker]` tokens from them, top to bottom.
/// A comment that does not start its line (e.g. trailing a previous item)
/// ends the block (FR-064). `parsed` is the string the tree was parsed from;
/// `text_of` yields the text of a node (Kotlin reads the original source, not
/// its ASCII shadow). Nodes for which `is_doc` holds are read over without
/// contributing markers and without ending the block.
fn collect_preceding_markers_with<'a>(
    children: &[tree_sitter::Node],
    target_idx: usize,
    parsed: &str,
    text_of: &dyn Fn(tree_sitter::Node) -> Option<&'a str>,
    comment_kinds: &[&str],
    is_doc: &dyn Fn(tree_sitter::Node) -> bool,
) -> Vec<String> {
    let mut next_start_row = children[target_idx].start_position().row;
    let mut j = target_idx as isize - 1;
    let mut comments = Vec::new();
    while j >= 0 {
        let node = children[j as usize];
        if !comment_kinds.contains(&node.kind()) {
            break;
        }
        let Some(text) = text_of(node) else {
            break;
        };
        if comment_end_row(node) + 1 != next_start_row || !is_own_line_comment(parsed, node) {
            break;
        }
        if !is_doc(node) {
            comments.push(text);
        }
        next_start_row = node.start_position().row;
        j -= 1;
    }
    comments.reverse();
    let mut markers = Vec::new();
    for text in comments {
        markers.extend(extract_markers(text));
    }
    markers
}

/// Single-kind variant of [`collect_preceding_markers_with`] (no doc skip).
fn collect_preceding_markers(
    children: &[tree_sitter::Node],
    target_idx: usize,
    source: &str,
    comment_kind: &str,
) -> Vec<String> {
    collect_preceding_markers_with(
        children,
        target_idx,
        source,
        &|n| n.utf8_text(source.as_bytes()).ok(),
        &[comment_kind],
        &|_| false,
    )
}

/// Rust doc comments (`///`, `//!`, `/** */`, `/*! */`) carry an
/// `outer_doc_comment_marker` / `inner_doc_comment_marker` child. Decided by
/// node structure so `////`, `/***` and `/**/` stay ordinary comments.
fn is_rust_doc_comment(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|c| {
        matches!(
            c.kind(),
            "outer_doc_comment_marker" | "inner_doc_comment_marker"
        )
    });
    found
}

/// Returns `None` if the attribute chain above `children[func_idx]` has no
/// test attribute. The chain is the contiguous run of `attribute_item`s and
/// comments directly above the `fn` (comments do not end the chain). A test
/// attribute is one whose path's last segment is `test` or listed in
/// `extra_attributes`. Otherwise returns the markers from the comment block
/// immediately (no blank line) preceding the topmost test attribute.
fn rust_test_markers(
    children: &[tree_sitter::Node],
    func_idx: usize,
    source: &str,
    extra_attributes: &[String],
) -> Option<Vec<String>> {
    let mut test_attr_idx: Option<usize> = None;
    let mut j = func_idx as isize - 1;
    while j >= 0 {
        let node = children[j as usize];
        if node.kind() == "attribute_item" {
            if rust_is_test_attribute(node, source, extra_attributes) {
                test_attr_idx = Some(j as usize);
            }
        } else if !RUST_COMMENT_KINDS.contains(&node.kind()) {
            break;
        }
        j -= 1;
    }
    let test_attr_idx = test_attr_idx?;
    Some(collect_preceding_markers_with(
        children,
        test_attr_idx,
        source,
        &|n| n.utf8_text(source.as_bytes()).ok(),
        &RUST_COMMENT_KINDS,
        &is_rust_doc_comment,
    ))
}

/// Whether an `attribute_item` is a test attribute. The attribute path's last
/// segment is the text of the first named child of `attribute` when it is an
/// `identifier`, or the `name` field when it is a `scoped_identifier`;
/// arguments are never inspected. It matches when that segment is `test` or
/// equals one of `extra_attributes`. `#![test]` is an `inner_attribute_item`,
/// so it never reaches this function.
fn rust_is_test_attribute(
    item: tree_sitter::Node,
    source: &str,
    extra_attributes: &[String],
) -> bool {
    let mut cursor = item.walk();
    let Some(attribute) = item
        .named_children(&mut cursor)
        .find(|c| c.kind() == "attribute")
    else {
        return false;
    };
    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    let segment = match path.kind() {
        "identifier" => path,
        "scoped_identifier" => match path.child_by_field_name("name") {
            Some(n) => n,
            None => return false,
        },
        _ => return false,
    };
    let text = segment.utf8_text(source.as_bytes()).unwrap_or("");
    text == "test" || extra_attributes.iter().any(|a| a == text)
}

const RUST_COMMENT_KINDS: [&str; 2] = ["line_comment", "block_comment"];

/// Doc comments (Javadoc / KDoc) are never markers in Java / Kotlin (and do
/// not end the adjacent comment block).
fn is_doc_comment(text: &str) -> bool {
    text.starts_with("/**")
}

/// Test names that are empty or contain control characters / line separators
/// (U+0085, U+2028, U+2029) are ignored.
fn is_valid_test_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}'))
}

/// Strips the comment syntax once per kind and returns the body together with
/// whether it is a block comment body. Line comments lose `//` and at most one
/// more `/` (`///`); block comments lose `/*`, at most one more `*` (`/**`) and
/// the closing `*/`. Any further `/` or `*` stays in the body.
fn comment_body(text: &str) -> (&str, bool) {
    if let Some(rest) = text.strip_prefix("//") {
        return (rest.strip_prefix('/').unwrap_or(rest), false);
    }
    match text.strip_prefix("/*") {
        Some(rest) => {
            let rest = rest.strip_suffix("*/").unwrap_or(rest);
            (rest.strip_prefix('*').unwrap_or(rest), true)
        }
        None => (text, false),
    }
}

/// Returns the markers of one comment. The comment is a marker comment only
/// when its body (comment syntax and surrounding whitespace removed) consists
/// solely of whitespace-separated `[marker]` tokens, where `marker` is one or
/// more `[A-Za-z0-9._-]` characters. Any other text makes it a plain comment
/// with no markers. FR-121.
fn extract_markers(text: &str) -> Vec<String> {
    let (body, is_block) = comment_body(text);
    let mut markers = Vec::new();
    for (i, line) in body.lines().enumerate() {
        // Continuation lines of a block comment may start with one `*`.
        let line = match (is_block && i > 0, line.trim_start().strip_prefix('*')) {
            (true, Some(rest)) => rest,
            _ => line,
        };
        for token in line.split_whitespace() {
            let Some(inner) = token
                .strip_prefix('[')
                .and_then(|t| t.strip_suffix(']'))
                .filter(|inner| {
                    !inner.is_empty()
                        && inner
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
                })
            else {
                return Vec::new();
            };
            markers.push(inner.to_string());
        }
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

/// File extensions (without the dot) of TypeScript / JavaScript (Vitest)
/// test files: `<name>.test.<ext>` / `<name>.spec.<ext>`.
pub const TYPESCRIPT_TEST_EXTENSIONS: [&str; 8] =
    ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Whether `path` is a TypeScript / JavaScript (Vitest) test file, i.e. ends
/// with `.test.<ext>` or `.spec.<ext>` for one of the supported extensions.
pub fn is_typescript_test_file(path: &str) -> bool {
    TYPESCRIPT_TEST_EXTENSIONS.iter().any(|ext| {
        path.ends_with(&format!(".test.{ext}")) || path.ends_with(&format!(".spec.{ext}"))
    })
}

/// Extract TypeScript / JavaScript (Vitest) tests from a `*.test.<ext>` /
/// `*.spec.<ext>` file (see [`TYPESCRIPT_TEST_EXTENSIONS`]): `it` / `test`
/// call statements at the top level or directly inside `describe` callbacks.
/// Markers come from the comment block immediately preceding the call, and
/// `content` is the whole call statement.
pub fn extract_typescript_tests(
    path: &str,
    source: &str,
) -> Result<Vec<ExtractedTest>, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = if [".ts", ".mts", ".cts"]
        .iter()
        .any(|ext| path.ends_with(ext))
    {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TSX.into()
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
    let base = TYPESCRIPT_TEST_EXTENSIONS
        .iter()
        .find_map(|ext| path.strip_suffix(&format!(".{ext}")))
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
    if !is_valid_test_name(name) {
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

// ---- Dart / Flutter (package:test / flutter_test) ----

/// Whether `path` is a Dart test file, i.e. ends with `_test.dart`.
pub fn is_dart_test_file(path: &str) -> bool {
    path.ends_with("_test.dart")
}

/// Dart doc comments (`///`, `/** */`) are never markers and do not end the
/// adjacent comment block. Decided by text so `////`, `/***` and `/**/` stay
/// ordinary comments (as for Rust).
fn is_dart_doc_comment(text: &str) -> bool {
    (text.starts_with("///") && !text.starts_with("////"))
        || (text.starts_with("/**") && !text.starts_with("/***") && !text.starts_with("/**/"))
}

/// Extract Dart (`package:test` / `flutter_test`) tests from a `*_test.dart`
/// file: `test` / `testWidgets` call statements directly inside `main` or
/// inside the callback of a `group` that is itself reachable that way.
/// Markers come from the comment block immediately preceding the call, and
/// `content` is the whole call statement.
pub fn extract_dart_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_dart::LANGUAGE.into())
        .expect("valid dart grammar");
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
    let base = path.strip_suffix(".dart").unwrap_or(path);
    let mut results = Vec::new();
    let mut cursor = root.walk();
    for decl in root.children(&mut cursor) {
        if decl.kind() != "function_declaration" {
            continue;
        }
        let is_main = decl
            .child_by_field_name("signature")
            .and_then(|s| s.child_by_field_name("name"))
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            == Some("main");
        if !is_main {
            continue;
        }
        let Some(block) = decl
            .child_by_field_name("body")
            .and_then(|b| b.named_child(0))
            .filter(|b| b.kind() == "block")
        else {
            continue;
        };
        walk_dart_statements(block, source, base, &[], &mut results);
    }
    Ok(results)
}

/// Positional arguments of a Dart `arguments` node (named arguments such as
/// `skip:` are left out).
fn dart_positional_args(args: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .filter(|n| n.kind() != "named_argument" && n.kind() != "comment")
        .collect()
}

/// Name from a single-line (possibly raw, possibly adjacent-concatenated)
/// `string_literal`: the source text inside each part's quotes, joined. Strings
/// with interpolation or triple quotes yield `None`; escapes are not
/// interpreted.
fn dart_string_name(lit: tree_sitter::Node, source: &str) -> Option<String> {
    if lit.kind() != "string_literal" {
        return None;
    }
    let mut name = String::new();
    let mut cursor = lit.walk();
    let mut parts = 0;
    for part in lit.children(&mut cursor) {
        let prefix = match part.kind() {
            "string_literal_single_quotes" | "string_literal_double_quotes" => 1,
            "raw_string_literal_single_quotes" | "raw_string_literal_double_quotes" => 2,
            "comment" => continue,
            _ => return None,
        };
        let mut c = part.walk();
        if part
            .children(&mut c)
            .any(|ch| ch.kind() == "template_substitution")
        {
            return None;
        }
        let text = part.utf8_text(source.as_bytes()).ok()?;
        if text.len() < prefix + 1 {
            return None;
        }
        name.push_str(text.get(prefix..text.len() - 1)?);
        parts += 1;
    }
    if parts == 0 || !is_valid_test_name(&name) {
        return None;
    }
    Some(name)
}

fn walk_dart_statements(
    block: tree_sitter::Node,
    source: &str,
    base: &str,
    groups: &[String],
    results: &mut Vec<ExtractedTest>,
) {
    let mut cursor = block.walk();
    let children: Vec<tree_sitter::Node> = block.children(&mut cursor).collect();
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
        if callee.kind() != "identifier" {
            continue;
        }
        let head = callee.utf8_text(source.as_bytes()).unwrap_or("");
        if !matches!(head, "test" | "testWidgets" | "group") {
            continue;
        }
        let positional = dart_positional_args(args);
        let Some(name) = positional
            .first()
            .and_then(|a| dart_string_name(*a, source))
        else {
            continue;
        };
        if head == "group" {
            let Some(cb) = positional
                .get(1)
                .filter(|cb| cb.kind() == "function_expression")
            else {
                continue;
            };
            let Some(inner) = cb
                .child_by_field_name("body")
                .filter(|b| b.kind() == "function_expression_body")
                .and_then(|b| b.named_child(0))
                .filter(|b| b.kind() == "block")
            else {
                continue;
            };
            let mut new_groups = groups.to_vec();
            new_groups.push(name);
            walk_dart_statements(inner, source, base, &new_groups, results);
        } else {
            let markers = collect_preceding_markers_with(
                &children,
                idx,
                source,
                &|n| n.utf8_text(source.as_bytes()).ok(),
                &["comment"],
                &|n| {
                    n.utf8_text(source.as_bytes())
                        .is_ok_and(is_dart_doc_comment)
                },
            );
            let content = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            results.push(ExtractedTest {
                id: format!("{base}:{}:{name}", groups.join("/")),
                name,
                markers,
                content,
            });
        }
    }
}

// ---- Java / Kotlin (JUnit) ----

/// Annotation names (last identifier, qualifiers ignored) marking a JUnit
/// style test method / function in Java and Kotlin.
const JVM_TEST_ANNOTATIONS: [&str; 5] = [
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];

const JVM_COMMENT_KINDS: [&str; 2] = ["line_comment", "block_comment"];

/// Start byte of the hashed range: the first token of the declaration that is
/// neither an annotation nor a comment (annotations / comments that lead the
/// modifiers and the comments right after them are skipped).
fn jvm_content_start(decl: tree_sitter::Node) -> Option<usize> {
    fn skip(kind: &str) -> bool {
        matches!(
            kind,
            "annotation" | "marker_annotation" | "line_comment" | "block_comment"
        )
    }
    let mut cursor = decl.walk();
    for child in decl.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mut c2 = child.walk();
            let first = child.children(&mut c2).find(|n| !skip(n.kind()));
            if let Some(first) = first {
                return Some(first.start_byte());
            }
        } else if !skip(child.kind()) {
            return Some(child.start_byte());
        }
    }
    None
}

fn jvm_name_is_test(name: &str) -> bool {
    JVM_TEST_ANNOTATIONS.contains(&name)
}

// -- Java --

/// Extract Java (JUnit) tests: methods annotated with one of
/// [`JVM_TEST_ANNOTATIONS`] directly inside class / record / interface / enum
/// bodies. Markers come from the `//` / `/* */` comment block immediately
/// preceding the method; `content` starts at the first non-annotation token.
pub fn extract_java_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("valid java grammar");
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
    let base = path.strip_suffix(".java").unwrap_or(path);
    let mut results = Vec::new();
    walk_java_container(root, source, base, &[], &mut results);
    Ok(results)
}

fn walk_java_container(
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
            "class_declaration"
            | "record_declaration"
            | "interface_declaration"
            | "enum_declaration" => {
                let (Some(name_node), Some(body)) = (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("body"),
                ) else {
                    continue;
                };
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                let mut new_types = types.to_vec();
                new_types.push(name.to_string());
                if body.kind() == "enum_body" {
                    let mut c = body.walk();
                    let decls: Vec<_> = body
                        .children(&mut c)
                        .filter(|n| n.kind() == "enum_body_declarations")
                        .collect();
                    for d in decls {
                        walk_java_container(d, source, base, &new_types, results);
                    }
                } else {
                    walk_java_container(body, source, base, &new_types, results);
                }
            }
            "method_declaration" if !types.is_empty() => {
                if !java_is_test_method(*node, source) {
                    continue;
                }
                let Some(name_node) = node.child_by_field_name("name") else {
                    continue;
                };
                let Some(start) = jvm_content_start(*node) else {
                    continue;
                };
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                results.push(ExtractedTest {
                    id: format!("{base}:{}:{name}", types.join("/")),
                    name,
                    markers: collect_preceding_markers_with(
                        &children,
                        idx,
                        source,
                        &|n| n.utf8_text(source.as_bytes()).ok(),
                        &JVM_COMMENT_KINDS,
                        &|n| n.utf8_text(source.as_bytes()).is_ok_and(is_doc_comment),
                    ),
                    content: source[start..node.end_byte()].to_string(),
                });
            }
            _ => {}
        }
    }
}

fn java_is_test_method(method: tree_sitter::Node, source: &str) -> bool {
    let mut cursor = method.walk();
    let Some(modifiers) = method
        .children(&mut cursor)
        .find(|n| n.kind() == "modifiers")
    else {
        return false;
    };
    let mut c2 = modifiers.walk();
    for ann in modifiers.children(&mut c2) {
        if ann.kind() != "marker_annotation" && ann.kind() != "annotation" {
            continue;
        }
        let Some(mut name) = ann.child_by_field_name("name") else {
            continue;
        };
        while name.kind() == "scoped_identifier" {
            match name.child_by_field_name("name") {
                Some(n) => name = n,
                None => break,
            }
        }
        if jvm_name_is_test(name.utf8_text(source.as_bytes()).unwrap_or("")) {
            return true;
        }
    }
    false
}

// -- Kotlin --

/// The Kotlin grammar rejects identifiers with many non-Latin characters.
/// The source is therefore parsed as an "ASCII shadow": every non-ASCII
/// code point becomes one ASCII character (`' '` for whitespace and U+FEFF,
/// `'x'` otherwise). `map` records `(shadow_offset, original_offset)` pairs at
/// every point where the two offsets diverge, so shadow byte ranges can be
/// mapped back to the original source.
struct KotlinShadow<'a> {
    original: &'a str,
    shadow: String,
    map: Vec<(usize, usize)>,
}

impl<'a> KotlinShadow<'a> {
    fn new(original: &'a str) -> Self {
        let mut shadow = String::with_capacity(original.len());
        let mut map = vec![(0usize, 0usize)];
        for (off, ch) in original.char_indices() {
            if ch.is_ascii() {
                shadow.push(ch);
            } else {
                shadow.push(if ch.is_whitespace() || ch == '\u{feff}' {
                    ' '
                } else {
                    'x'
                });
                map.push((shadow.len(), off + ch.len_utf8()));
            }
        }
        KotlinShadow {
            original,
            shadow,
            map,
        }
    }

    /// Replaces the given shadow byte ranges with spaces (newlines kept).
    fn blank_ranges(&mut self, ranges: &[(usize, usize)]) {
        let mut bytes = std::mem::take(&mut self.shadow).into_bytes();
        for &(a, b) in ranges {
            for byte in &mut bytes[a..b] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        self.shadow = String::from_utf8(bytes).expect("ascii shadow");
    }

    fn to_original(&self, shadow_offset: usize) -> usize {
        let i = self.map.partition_point(|&(s, _)| s <= shadow_offset) - 1;
        let (s, o) = self.map[i];
        o + (shadow_offset - s)
    }

    fn slice(&self, start: usize, end: usize) -> Option<&'a str> {
        self.original
            .get(self.to_original(start)..self.to_original(end))
    }

    fn node_text(&self, node: tree_sitter::Node) -> Option<&'a str> {
        self.slice(node.start_byte(), node.end_byte())
    }
}

/// Extract Kotlin (JUnit) tests: functions annotated with one of
/// [`JVM_TEST_ANNOTATIONS`] at the top level or directly inside class /
/// object / companion object bodies. Parsing uses an ASCII shadow of the
/// source (see [`KotlinShadow`]); names and `content` come from the original.
pub fn extract_kotlin_tests(path: &str, source: &str) -> Result<Vec<ExtractedTest>, ExtractError> {
    kotlin_extract(path, source, true)
}

/// Top-level node kinds the Kotlin grammar may legitimately produce under
/// `source_file`. Anything else (e.g. a leftover `annotated_expression`)
/// means the grammar failed to recognise a declaration.
const KOTLIN_ROOT_KINDS: [&str; 11] = [
    "shebang",
    "file_annotation",
    "package_header",
    "import",
    "class_declaration",
    "object_declaration",
    "function_declaration",
    "property_declaration",
    "type_alias",
    "line_comment",
    "block_comment",
];

/// `retry_annotated_expression` enables the class-annotation blanking
/// workaround (disabled only by tests of the fallback error path).
fn kotlin_extract(
    path: &str,
    source: &str,
    retry_annotated_expression: bool,
) -> Result<Vec<ExtractedTest>, ExtractError> {
    let parse_err = || ExtractError::Parse {
        path: path.to_string(),
    };
    let mut shadow = KotlinShadow::new(source);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("valid kotlin grammar");
    let mut tree = parser.parse(&shadow.shadow, None).ok_or_else(parse_err)?;
    if tree.root_node().has_error() {
        return Err(parse_err());
    }
    // Grammar bugs that leave an `annotated_expression` at file level with no
    // error: (1) a class with 2+ annotations, no supertype and an
    // `@Annotation inner class` in its body; (2) an annotation with
    // positional arguments (`@S("L")`) after a top-level declaration and a
    // comment. The annotations are not needed for extraction, so blank them
    // out and parse again.
    if retry_annotated_expression && kotlin_has_annotated_expression(tree.root_node()) {
        let ranges = kotlin_annotated_expression_ranges(tree.root_node());
        drop(tree);
        shadow.blank_ranges(&ranges);
        tree = parser.parse(&shadow.shadow, None).ok_or_else(parse_err)?;
        if tree.root_node().has_error() {
            return Err(parse_err());
        }
    }
    let root = tree.root_node();
    // A structure other than declarations at file level means the grammar
    // missed a declaration: never silently report zero tests.
    let mut cursor = root.walk();
    let unexpected = root
        .children(&mut cursor)
        .any(|n| !KOTLIN_ROOT_KINDS.contains(&n.kind()));
    if unexpected {
        return Err(parse_err());
    }
    let base = path.strip_suffix(".kt").unwrap_or(path);
    let mut results = Vec::new();
    walk_kotlin_container(root, &shadow, base, &[], &mut results).map_err(|()| parse_err())?;
    Ok(results)
}

fn kotlin_has_annotated_expression(root: tree_sitter::Node) -> bool {
    let mut cursor = root.walk();
    let found = root
        .children(&mut cursor)
        .any(|n| n.kind() == "annotated_expression");
    found
}

/// Byte ranges to blank for each file-level `annotated_expression`. With
/// positional arguments (`@S("L")`) the grammar splits the annotation into an
/// `annotation` (`@S`) and a `parenthesized_expression` (`("L")`); only the
/// latter is blanked, which leaves a plain annotation that parses as part of
/// the declaration's modifiers (so marker adjacency still sees its first
/// row). Otherwise (the class case) the `annotation` children are blanked. The
/// expression as a whole is never blanked: in the class case it spans the
/// entire class body.
fn kotlin_annotated_expression_ranges(root: tree_sitter::Node) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    for expr in root.children(&mut cursor) {
        if expr.kind() != "annotated_expression" {
            continue;
        }
        let mut c2 = expr.walk();
        let children: Vec<tree_sitter::Node> = expr.children(&mut c2).collect();
        let args: Vec<_> = children
            .windows(2)
            .filter(|w| w[0].kind() == "annotation" && w[1].kind() == "parenthesized_expression")
            .map(|w| &w[1])
            .collect();
        let picked: Vec<_> = if args.is_empty() {
            children
                .iter()
                .filter(|c| c.kind() == "annotation")
                .collect()
        } else {
            args
        };
        ranges.extend(picked.iter().map(|n| (n.start_byte(), n.end_byte())));
    }
    ranges
}

fn kotlin_ident(text: &str) -> String {
    text.strip_prefix('`')
        .and_then(|t| t.strip_suffix('`'))
        .unwrap_or(text)
        .to_string()
}

fn walk_kotlin_container(
    container: tree_sitter::Node,
    shadow: &KotlinShadow,
    base: &str,
    types: &[String],
    results: &mut Vec<ExtractedTest>,
) -> Result<(), ()> {
    let mut cursor = container.walk();
    let children: Vec<tree_sitter::Node> = container.children(&mut cursor).collect();
    for (idx, node) in children.iter().enumerate() {
        match node.kind() {
            "class_declaration" | "object_declaration" | "companion_object" => {
                let name = match node.child_by_field_name("name") {
                    Some(n) => match shadow.node_text(n) {
                        Some(t) => kotlin_ident(t),
                        None => {
                            debug_assert!(false, "kotlin name range off a char boundary");
                            return Err(());
                        }
                    },
                    None if node.kind() == "companion_object" => "Companion".to_string(),
                    None => continue,
                };
                let mut c = node.walk();
                let Some(body) = node
                    .children(&mut c)
                    .find(|n| matches!(n.kind(), "class_body" | "enum_class_body"))
                else {
                    continue;
                };
                let mut new_types = types.to_vec();
                new_types.push(name);
                walk_kotlin_container(body, shadow, base, &new_types, results)?;
            }
            "function_declaration" => {
                if !kotlin_is_test_function(*node, shadow) {
                    continue;
                }
                let Some(name_node) = node.child_by_field_name("name") else {
                    continue;
                };
                let Some(name_text) = shadow.node_text(name_node) else {
                    debug_assert!(false, "kotlin name range off a char boundary");
                    return Err(());
                };
                let name = kotlin_ident(name_text);
                if !is_valid_test_name(&name) {
                    continue;
                }
                let Some(start) = jvm_content_start(*node) else {
                    continue;
                };
                let Some(content) = shadow.slice(start, node.end_byte()) else {
                    debug_assert!(false, "kotlin content range off a char boundary");
                    return Err(());
                };
                results.push(ExtractedTest {
                    id: format!("{base}:{}:{name}", types.join("/")),
                    name,
                    // Markers are read from the original source: the shadow
                    // turns `[正常系]` into a bogus `[xxx]`.
                    markers: collect_preceding_markers_with(
                        &children,
                        idx,
                        &shadow.shadow,
                        &|n| shadow.node_text(n),
                        &JVM_COMMENT_KINDS,
                        &|n| shadow.node_text(n).is_some_and(is_doc_comment),
                    ),
                    content: content.to_string(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn kotlin_is_test_function(func: tree_sitter::Node, shadow: &KotlinShadow) -> bool {
    let mut cursor = func.walk();
    let Some(modifiers) = func.children(&mut cursor).find(|n| n.kind() == "modifiers") else {
        return false;
    };
    let mut c2 = modifiers.walk();
    let found = modifiers
        .children(&mut c2)
        .filter(|n| n.kind() == "annotation")
        .any(|ann| kotlin_annotation_has_test(ann, shadow));
    found
}

/// Whether any `user_type` under the annotation (outside argument lists) ends
/// in a test annotation name.
fn kotlin_annotation_has_test(node: tree_sitter::Node, shadow: &KotlinShadow) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "user_type" => {
                let mut c = child.walk();
                let last = child
                    .children(&mut c)
                    .filter(|n| matches!(n.kind(), "identifier" | "type_identifier"))
                    .last();
                if let Some(last) = last {
                    if jvm_name_is_test(shadow.node_text(last).unwrap_or("")) {
                        return true;
                    }
                }
            }
            "value_arguments" | "use_site_target" => {}
            _ => {
                if kotlin_annotation_has_test(child, shadow) {
                    return true;
                }
            }
        }
    }
    false
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
            &extract_rust_tests("src/auth.rs", base, &[]).unwrap(),
            "rejects_short_password",
        )
        .content
        .clone();
        let b = find(
            &extract_rust_tests("src/auth.rs", changed, &[]).unwrap(),
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
        let extracted = extract_rust_tests("src/auth.rs", source, &[]).unwrap();
        let t = find(&extracted, "rejects_short_password");
        assert!(!t.content.contains("#[test]"));
        assert!(!t.content.contains("[FR-001]"));
        assert!(t.content.contains("fn rejects_short_password"));
    }

    fn rs(source: &str, extra: &[&str]) -> Vec<ExtractedTest> {
        let extra: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
        extract_rust_tests("src/auth.rs", source, &extra).unwrap()
    }

    fn has(tests: &[ExtractedTest], name: &str) -> bool {
        tests.iter().any(|t| t.name == name)
    }

    // [FR-015]
    #[test]
    fn rustは属性パスの末尾がtestならtestとして抽出する() {
        let source = r#"// [FR-001]
#[test]
fn plain_test() {}

// [FR-002]
#[tokio::test]
async fn async_test() {}

// [FR-003]
#[async_std::test]
async fn async_std_test() {}

// [FR-004]
#[tokio::test(flavor = "multi_thread")]
async fn multi_thread_test() {}

// [FR-005]
#[ test ]
fn spaced_test() {}
"#;
        let tests = rs(source, &[]);
        for name in [
            "plain_test",
            "async_test",
            "async_std_test",
            "multi_thread_test",
            "spaced_test",
        ] {
            assert!(has(&tests, name), "{name} should be extracted");
        }
        assert_eq!(
            find(&tests, "async_test").markers,
            vec!["FR-002".to_string()]
        );
    }

    // [FR-015]
    #[test]
    fn rustのrstestは追加指定がなければ対象外で指定があれば対象になる() {
        let source = "// [FR-001]\n#[rstest]\nfn t() {}\n";
        assert!(!has(&rs(source, &[]), "t"));
        assert!(has(&rs(source, &["rstest"]), "t"));

        let scoped = "// [FR-001]\n#[rstest::rstest]\nfn t() {}\n";
        assert!(!has(&rs(scoped, &[]), "t"));
        assert!(has(&rs(scoped, &["rstest"]), "t"));
    }

    // [FR-015]
    #[test]
    fn rustのtest_caseとcfg_attrのtestは対象外() {
        let source = r#"// [FR-001]
#[test_case(1)]
fn case_test() {}

// [FR-002]
#[cfg_attr(x, test)]
fn cfg_attr_test() {}
"#;
        let tests = rs(source, &[]);
        assert!(!has(&tests, "case_test"));
        assert!(!has(&tests, "cfg_attr_test"));

        let tests = rs(source, &["test_case"]);
        assert!(has(&tests, "case_test"));
        assert!(!has(&tests, "cfg_attr_test"));
    }

    // [FR-015]
    #[test]
    fn rustのinner_attributeのtestはtestを作らない() {
        let source = "#![test]\n\n// [FR-001]\nfn helper() {}\n";
        assert!(rs(source, &[]).is_empty());
    }

    // [FR-016]
    #[test]
    fn rustは最初の対象属性の直前のコメントをマーカーにする() {
        let source = "// [FR-001]\n#[rstest]\n#[case(1)]\n#[tokio::test]\nfn t() {}\n";
        let tests = rs(source, &["rstest"]);
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
    }

    // [FR-016]
    #[test]
    fn rustは対象属性の設定によりマーカー位置が変わる() {
        let source = "#[rstest]\n// [FR-001]\n#[tokio::test]\nfn t() {}\n";
        let without = rs(source, &[]);
        assert_eq!(find(&without, "t").markers, vec!["FR-001".to_string()]);
        let with = rs(source, &["rstest"]);
        assert!(find(&with, "t").markers.is_empty());
    }

    // [FR-016]
    #[test]
    fn rustは属性間や属性末尾のコメントがあってもtestとして検出しマーカーを取る() {
        let between = "// [FR-001]\n#[test]\n// note\n#[ignore]\nfn between() {}\n";
        let tests = rs(between, &[]);
        assert_eq!(find(&tests, "between").markers, vec!["FR-001".to_string()]);

        let trailing = "// [FR-001]\n#[test] // trailing\n#[ignore]\nfn trailing() {}\n";
        let tests = rs(trailing, &[]);
        assert_eq!(find(&tests, "trailing").markers, vec!["FR-001".to_string()]);

        let block = "// [FR-001]\n#[test]\n/* block */\n#[ignore]\nfn block() {}\n";
        assert_eq!(
            find(&rs(block, &[]), "block").markers,
            vec!["FR-001".to_string()]
        );
    }

    // [FR-004]
    #[test]
    fn tokioのtest属性はcontentに含まれない() {
        let source = "// [FR-001]\n#[tokio::test]\nasync fn t() { assert!(true); }\n";
        let tests = rs(source, &[]);
        let t = find(&tests, "t");
        assert!(!t.content.contains("tokio::test"));
        assert!(!t.content.contains("#["));
        assert!(t.content.contains("async fn t"));
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
        let tests = extract_rust_tests("src/auth.rs", source, &[]).unwrap();
        assert_eq!(find(&tests, "adjacent").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "blank_line_between").markers.is_empty());
        assert!(find(&tests, "no_comment").markers.is_empty());
    }

    // [FR-017]
    #[test]
    fn rustのtest識別子はトップレベルなら空モジュールパスになる() {
        let source = "// [FR-001]\n#[test]\nfn roundtrip() {}\n";
        let tests = extract_rust_tests("src/lock.rs", source, &[]).unwrap();
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
        let tests = extract_rust_tests("src/lock.rs", source, &[]).unwrap();
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
        let tests = extract_rust_tests("src/x.rs", source, &[]).unwrap();
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
        let tests = extract_rust_tests("src/auth.rs", source, &[]).unwrap();
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
        let tests = extract_rust_tests("src/auth.rs", source, &[]).unwrap();
        assert_eq!(
            find(&tests, "adjacent_after_other_attr").markers,
            vec!["FR-001".to_string()]
        );
    }

    // [FR-016]
    #[test]
    fn rustはtestとfnの間にコメントがあってもtestとして検出しマーカーを採用する() {
        let source = r#"// [FR-001]
#[test]
// 補足
fn t() {}
"#;
        let tests = extract_rust_tests("src/auth.rs", source, &[]).unwrap();
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

    // ---- TypeScript / JavaScript ----

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
    fn typescriptのtest識別子は対象拡張子をすべて除去しtestやspecは残す() {
        let src = "it(\"t\", () => {});\n";
        for (path, id) in [
            ("src/a.test.ts", "src/a.test::t"),
            ("src/a.spec.tsx", "src/a.spec::t"),
            ("src/a.test.mjs", "src/a.test::t"),
            ("src/a.spec.cts", "src/a.spec::t"),
            ("src/a.test.jsx", "src/a.test::t"),
            ("src/a.test.js", "src/a.test::t"),
            ("src/a.spec.cjs", "src/a.spec::t"),
            ("src/a.test.mts", "src/a.test::t"),
        ] {
            let tests = extract_typescript_tests(path, src).unwrap();
            assert_eq!(tests[0].id, id, "{path}");
        }
    }

    // [FR-056]
    #[test]
    fn typescriptのtest識別子は対象外の拡張子を除去しない() {
        let tests =
            extract_typescript_tests("src/a.test.ts.bak", "it(\"t\", () => {});\n").unwrap();
        assert_eq!(tests[0].id, "src/a.test.ts.bak::t");
    }

    // [FR-053]
    #[test]
    fn typescriptはjs系拡張子のファイルからtestを検出する() {
        for path in [
            "src/a.test.js",
            "src/a.spec.mjs",
            "src/a.test.cjs",
            "src/a.spec.mts",
            "src/a.test.cts",
        ] {
            let tests = extract_typescript_tests(path, "it(\"t\", () => {});\n").unwrap();
            assert_eq!(tests.len(), 1, "{path}");
        }
    }

    // [FR-053]
    #[test]
    fn typescriptはts系拡張子でtsx文法が拒否する角括弧キャストを解析できる() {
        let source = "it(\"t\", () => {\n  const n = <number>x;\n});\n";
        for path in ["src/a.test.ts", "src/a.test.mts", "src/a.spec.cts"] {
            let tests = extract_typescript_tests(path, source).unwrap();
            assert_eq!(tests.len(), 1, "{path}");
        }
    }

    // [FR-053]
    #[test]
    fn typescriptはjsxとjsファイルのjsxを含むソースも解析できる() {
        let source = "it(\"renders\", () => {\n  render(<App />);\n});\n";
        for path in ["src/a.test.jsx", "src/a.test.js"] {
            let tests = extract_typescript_tests(path, source).unwrap();
            assert_eq!(tests.len(), 1, "{path}");
            assert_eq!(tests[0].id, "src/a.test::renders", "{path}");
        }
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

    // ---- Java ----

    fn jv(source: &str) -> Vec<ExtractedTest> {
        extract_java_tests("tests/AuthTests.java", source).unwrap()
    }

    fn jv_wrap(body: &str) -> String {
        format!("public class AuthTests {{\n{body}\n}}\n")
    }

    // [FR-059]
    #[test]
    fn javaは5種類の注釈をそれぞれtestとして検出する() {
        for ann in [
            "Test",
            "ParameterizedTest",
            "RepeatedTest",
            "TestFactory",
            "TestTemplate",
        ] {
            let source = jv_wrap(&format!("    @{ann}\n    void t() {{ }}"));
            let tests = jv(&source);
            assert_eq!(tests.len(), 1, "annotation {ann} should be detected");
            assert_eq!(tests[0].name, "t");
        }
    }

    // [FR-059]
    #[test]
    fn javaは修飾名の注釈と引数付きの注釈も検出する() {
        let source = jv_wrap(
            "    @org.junit.jupiter.api.Test\n    void a() { }\n\n    @Test(timeout = 1)\n    void b() { }",
        );
        let tests = jv(&source);
        assert_eq!(tests.len(), 2);
        find(&tests, "a");
        find(&tests, "b");
    }

    // [FR-059]
    #[test]
    fn javaは無関係な注釈のみのメソッドを検出しない() {
        let source = jv_wrap(
            "    @Disabled\n    void a() { }\n\n    void b() { }\n\n    @Tag(\"x\")\n    void c() { }",
        );
        assert!(jv(&source).is_empty());
    }

    // [FR-059]
    #[test]
    fn javaは修飾子の後ろに書かれた注釈も検出する() {
        let source = jv_wrap("    public @Test void t() { }");
        let tests = jv(&source);
        assert_eq!(tests.len(), 1);
        assert!(tests[0].content.contains("@Test"), "{:?}", tests[0].content);
    }

    // [FR-060]
    #[test]
    fn javaは直前に隣接するコメントをマーカーとして抽出し空行を挟むと無視する() {
        let source = jv_wrap(
            "    // [FR-001]\n    @Test\n    void adjacent() { }\n\n    // [FR-002]\n\n    @Test\n    void blank() { }\n\n    @Test\n    void none() { }",
        );
        let tests = jv(&source);
        assert_eq!(find(&tests, "adjacent").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "blank").markers.is_empty());
        assert!(find(&tests, "none").markers.is_empty());
    }

    // [FR-060]
    #[test]
    fn javaはブロックコメントをマーカーにできる() {
        let source = jv_wrap("    /* [FR-002] */\n    @Test\n    void t() { }");
        assert_eq!(jv(&source)[0].markers, vec!["FR-002".to_string()]);
    }

    // [FR-060]
    #[test]
    fn javaはjavadocコメントをマーカーにしない() {
        let source = jv_wrap("    /** [FR-001] */\n    @Test\n    void t() { }");
        assert!(jv(&source)[0].markers.is_empty());
    }

    // [FR-060]
    #[test]
    fn javaは注釈と署名の間のコメントをマーカーにせずcontentにも含めない() {
        let source = jv_wrap("    @Test\n    // [FR-001]\n    // 補足\n    void t() { }");
        let tests = jv(&source);
        assert!(tests[0].markers.is_empty());
        assert!(!tests[0].content.contains("FR-001"));
        assert!(!tests[0].content.contains("補足"));
    }

    // [FR-061]
    #[test]
    fn javaのtest識別子はclassならファイルパスと型名とメソッド名になる() {
        let tests = jv(&jv_wrap("    @Test\n    void rejectsShort() { }"));
        assert_eq!(tests[0].id, "tests/AuthTests:AuthTests:rejectsShort");
    }

    // [FR-061]
    #[test]
    fn javaのtest識別子はネストした型を外側から内側へスラッシュで連結する() {
        let source = "public class Outer {\n    @Nested\n    class Inner {\n        @Test\n        void t() { }\n    }\n}\n";
        let tests = jv(source);
        assert_eq!(tests[0].id, "tests/AuthTests:Outer/Inner:t");
    }

    // [FR-061]
    #[test]
    fn javaのtest識別子はrecord_interface_enumの中でも型名を辿る() {
        let source = "record R() {\n    @Test\n    void a() { }\n}\n\ninterface I {\n    @Test\n    default void b() { }\n}\n\nenum E {\n    X, Y;\n\n    @Test\n    void c() { }\n}\n";
        let tests = jv(source);
        assert_eq!(find(&tests, "a").id, "tests/AuthTests:R:a");
        assert_eq!(find(&tests, "b").id, "tests/AuthTests:I:b");
        assert_eq!(find(&tests, "c").id, "tests/AuthTests:E:c");
    }

    // [FR-061]
    #[test]
    fn javaのtest識別子はpackageを含まない() {
        let source =
            "package com.example;\n\npublic class AuthTests {\n    @Test\n    void t() { }\n}\n";
        assert_eq!(jv(source)[0].id, "tests/AuthTests:AuthTests:t");
    }

    // [FR-059]
    #[test]
    fn javaはトップレベルメソッドを対象外にする() {
        let source = "@Test\nvoid t() { }\n";
        let tests = extract_java_tests("tests/AuthTests.java", source).unwrap();
        assert!(tests.is_empty());
        // a class alongside is still extracted
        let source = "class A {\n    @Test\n    void u() { }\n}\n";
        assert_eq!(jv(source).len(), 1);
    }

    // [FR-059]
    #[test]
    fn javaは匿名クラスとenum定数本体の中を対象外にする() {
        let source = "public class A {\n    void helper() {\n        Runnable r = new Runnable() {\n            @Test\n            public void run() { }\n        };\n    }\n\n    @Test\n    void real() { }\n}\n\nenum E {\n    X {\n        @Test\n        void inConst() { }\n    };\n\n    @Test\n    void outside() { }\n}\n";
        let tests = jv(source);
        let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["real", "outside"], "{names:?}");
    }

    // [FR-062]
    #[test]
    fn javaのcontentは注釈を含まず修飾子から始まる() {
        let source = jv_wrap(
            "    // [FR-001]\n    @Test\n    @Tag(\"a\")\n    public void t() { assertTrue(true); }",
        );
        let c = &jv(&source)[0].content;
        assert!(c.starts_with("public void t()"), "content = {c:?}");
        assert!(c.ends_with('}'));
        assert!(!c.contains("@Test"));
        assert!(!c.contains("@Tag"));
        assert!(!c.contains("FR-001"));
    }

    // [FR-062]
    #[test]
    fn javaのcontentは型パラメータ付きでも修飾子から始まる() {
        let source = jv_wrap("    @Test\n    static <T> void t() { }");
        let c = &jv(&source)[0].content;
        assert!(c.starts_with("static <T> void t()"), "content = {c:?}");
    }

    // [FR-062]
    #[test]
    fn javaのcontentは修飾子なしなら型パラメータから始まる() {
        let source = jv_wrap("    @Test\n    <T> void t() { }");
        let c = &jv(&source)[0].content;
        assert!(c.starts_with("<T> void t()"), "content = {c:?}");
    }

    // [FR-003]
    #[test]
    fn javaの本体変更はcontentに反映される() {
        let a = jv(&jv_wrap("    @Test\n    void t() { assertTrue(true); }"));
        let b = jv(&jv_wrap("    @Test\n    void t() { assertTrue(false); }"));
        assert_ne!(a[0].content, b[0].content);
    }

    // [FR-062]
    #[test]
    fn javaの注釈だけの変更はcontentを変えない() {
        let a = jv(&jv_wrap("    @Test\n    void t() { assertTrue(true); }"));
        let b = jv(&jv_wrap(
            "    @Test(timeout = 1)\n    @Tag(\"a\")\n    void t() { assertTrue(true); }",
        ));
        assert_eq!(a[0].content, b[0].content);
    }

    // [FR-014]
    #[test]
    fn javaは構文エラーのソースでエラーを返す() {
        let source = "public class A {\n    @Test\n    void t( {\n}\n";
        assert!(extract_java_tests("tests/A.java", source).is_err());
    }

    // ---- Kotlin ----

    fn kt(source: &str) -> Vec<ExtractedTest> {
        extract_kotlin_tests("tests/AuthTest.kt", source).unwrap()
    }

    fn kt_wrap(body: &str) -> String {
        format!("class AuthTest {{\n{body}\n}}\n")
    }

    // [FR-059]
    #[test]
    fn kotlinは5種類の注釈をそれぞれtestとして検出する() {
        for ann in [
            "Test",
            "ParameterizedTest",
            "RepeatedTest",
            "TestFactory",
            "TestTemplate",
        ] {
            let source = kt_wrap(&format!("    @{ann}\n    fun t() {{ }}"));
            let tests = kt(&source);
            assert_eq!(tests.len(), 1, "annotation {ann} should be detected");
            assert_eq!(tests[0].name, "t");
        }
    }

    // [FR-059]
    #[test]
    fn kotlinは修飾名と複数注釈リストとuse_site_target付きの注釈も検出する() {
        let source = kt_wrap(
            "    @org.junit.jupiter.api.Test\n    fun a() { }\n\n    @[Test Tag(\"a\")]\n    fun b() { }\n\n    @get:Test\n    fun c() { }",
        );
        let tests = kt(&source);
        assert_eq!(tests.len(), 3, "{tests:?}");
        find(&tests, "a");
        find(&tests, "b");
        find(&tests, "c");
    }

    // [FR-059]
    #[test]
    fn kotlinは無関係な注釈のみの関数を検出しない() {
        let source = kt_wrap("    @Disabled\n    fun a() { }\n\n    fun b() { }");
        assert!(kt(&source).is_empty());
    }

    // [FR-060]
    #[test]
    fn kotlinは直前に隣接するコメントをマーカーとして抽出し空行を挟むと無視する() {
        let source = kt_wrap(
            "    // [FR-001]\n    @Test\n    fun adjacent() { }\n\n    // [FR-002]\n\n    @Test\n    fun blank() { }\n\n    /* [FR-003] */\n    @Test\n    fun block() { }",
        );
        let tests = kt(&source);
        assert_eq!(find(&tests, "adjacent").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "blank").markers.is_empty());
        assert_eq!(find(&tests, "block").markers, vec!["FR-003".to_string()]);
    }

    // [FR-060]
    #[test]
    fn kotlinはkdocコメントをマーカーにしない() {
        let source = kt_wrap("    /** [Foo] */\n    @Test\n    fun t() { }");
        assert!(kt(&source)[0].markers.is_empty());
    }

    // [FR-060]
    #[test]
    fn kotlinは注釈と署名の間のコメントをマーカーにせずcontentにも含めない() {
        let source = kt_wrap(
            "    @Test // [FR-001]\n    fun a() { }\n\n    @Test\n    // [FR-002]\n    fun b() { }",
        );
        let tests = kt(&source);
        for name in ["a", "b"] {
            let t = find(&tests, name);
            assert!(t.markers.is_empty(), "{name}: {:?}", t.markers);
            assert!(!t.content.contains("FR-00"), "{name}: {:?}", t.content);
        }
    }

    // [FR-061]
    #[test]
    fn kotlinのバッククォート名はバッククォートを除いた名前になる() {
        let source = kt_wrap("    @Test\n    fun `一覧 取得 は 200`() { }");
        let tests = kt(&source);
        assert_eq!(tests[0].name, "一覧 取得 は 200");
        assert_eq!(tests[0].id, "tests/AuthTest:AuthTest:一覧 取得 は 200");
    }

    // [FR-061]
    #[test]
    fn kotlinのtest識別子はネストした型を外側から内側へスラッシュで連結する() {
        let source = "class AuthTest {\n    @Nested\n    inner class Inner {\n        @Test\n        fun t() { }\n    }\n}\n";
        assert_eq!(kt(source)[0].id, "tests/AuthTest:AuthTest/Inner:t");
    }

    // [FR-061]
    #[test]
    fn kotlinのtest識別子はobjectとcompanion_objectを辿る() {
        let source = "object O {\n    @Test\n    fun a() { }\n}\n\nclass C {\n    companion object {\n        @Test\n        fun b() { }\n    }\n\n    companion object Factory {\n        @Test\n        fun c() { }\n    }\n}\n";
        // two companion objects in one class is invalid Kotlin semantically but
        // syntactically fine; the grammar does not reject it.
        let tests = kt(source);
        assert_eq!(find(&tests, "a").id, "tests/AuthTest:O:a");
        assert_eq!(find(&tests, "b").id, "tests/AuthTest:C/Companion:b");
        assert_eq!(find(&tests, "c").id, "tests/AuthTest:C/Factory:c");
    }

    // [FR-061]
    #[test]
    fn kotlinのトップレベル関数は型部分が空の識別子になる() {
        let source = "@Test\nfun top() { }\n";
        let tests = kt(source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id, "tests/AuthTest::top");
    }

    // [FR-061]
    #[test]
    fn kotlinのtest識別子はpackageを含まない() {
        let source = "package com.example\n\nclass AuthTest {\n    @Test\n    fun t() { }\n}\n";
        assert_eq!(kt(source)[0].id, "tests/AuthTest:AuthTest:t");
    }

    // [FR-061]
    #[test]
    fn kotlinはコンストラクタ注釈付きの主コンストラクタを持つクラスでもtestを取れる() {
        let source = "@SpringBootTest\nclass X @Autowired constructor(private val a: Int) : Base(a) {\n    @Test\n    fun `日本語名`() { }\n}\n";
        let tests = kt(source);
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].id, "tests/AuthTest:X:日本語名");
    }

    // [FR-059]
    #[test]
    fn kotlinは関数本体内のローカル関数や匿名objectやenum定数本体やgetterを対象外にする() {
        let source = "class A {\n    @Test\n    fun real() {\n        @Test\n        fun local() { }\n    }\n\n    val x: Int\n        @Test\n        get() = 1\n\n    val r = object : Runnable {\n        @Test\n        fun inObj() { }\n    }\n}\n\nenum class E {\n    X {\n        @Test\n        fun inConst() { }\n    };\n\n    @Test\n    fun outside() { }\n}\n";
        let tests = kt(source);
        let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["real", "outside"], "{names:?}");
    }

    // [FR-063]
    #[test]
    fn kotlinは漢字の関数名と型名のtestを抽出し識別子とcontentが元の文字を保つ() {
        let source = "class AuthTest {\n    @Nested\n    inner class 一覧取得 {\n        @Test\n        fun 取得() { println(\"漢字\") }\n    }\n}\n";
        let tests = kt(source);
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].id, "tests/AuthTest:AuthTest/一覧取得:取得");
        assert_eq!(tests[0].name, "取得");
        assert!(tests[0].content.contains("取得"));
        assert!(tests[0].content.contains("漢字"));
        assert!(!tests[0].content.contains('x'), "{:?}", tests[0].content);
    }

    // [FR-063]
    #[test]
    fn kotlinはカタカナとハングルの関数名も抽出する() {
        let source = kt_wrap("    @Test\n    fun テスト() { }\n\n    @Test\n    fun 테스트() { }");
        let tests = kt(&source);
        assert_eq!(tests.len(), 2, "{tests:?}");
        find(&tests, "テスト");
        find(&tests, "테스트");
    }

    // [FR-063]
    #[test]
    fn kotlinは非asciiの文字リテラルを含むファイルも解析できる() {
        let source =
            kt_wrap("    @Test\n    fun t() {\n        val c = 'あ'\n        val d = '漢'\n    }");
        let tests = kt(&source);
        assert_eq!(tests.len(), 1);
        assert!(tests[0].content.contains("'あ'"));
    }

    // [FR-063]
    #[test]
    fn kotlinはbom付きファイルも解析できる() {
        let source = format!("\u{feff}{}", kt_wrap("    @Test\n    fun t() { }"));
        let tests = kt(&source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "t");
    }

    // [FR-062]
    #[test]
    fn kotlinのcontentは注釈を含まず修飾子から始まる() {
        let source = kt_wrap(
            "    // [FR-001]\n    @Test\n    internal suspend fun t() { assertTrue(true) }",
        );
        let c = &kt(&source)[0].content;
        assert!(c.starts_with("internal suspend fun t()"), "content = {c:?}");
        assert!(c.ends_with('}'));
        assert!(!c.contains("@Test"));
        assert!(!c.contains("FR-001"));
    }

    // [FR-062]
    #[test]
    fn kotlinのcontentは注釈のみならfunから始まる() {
        let source = kt_wrap("    @Test\n    @Tag(\"a\")\n    fun t() { }");
        let c = &kt(&source)[0].content;
        assert!(c.starts_with("fun t()"), "content = {c:?}");
    }

    // [FR-062]
    #[test]
    fn kotlinの修飾子の後ろに書かれた注釈はcontentに含まれる() {
        let source = kt_wrap("    internal @Test fun t() { }");
        let tests = kt(&source);
        assert_eq!(tests.len(), 1);
        assert!(
            tests[0].content.starts_with("internal"),
            "{:?}",
            tests[0].content
        );
        assert!(tests[0].content.contains("@Test"), "{:?}", tests[0].content);
    }

    // [FR-062]
    #[test]
    fn kotlinの式本体は終端までcontentに含まれる() {
        let source = kt_wrap("    @Test\n    fun t() = assertTrue(computeSomething())");
        let c = &kt(&source)[0].content;
        assert!(c.ends_with("computeSomething())"), "content = {c:?}");
    }

    // [FR-003]
    #[test]
    fn kotlinの本体変更はcontentに反映される() {
        let a = kt(&kt_wrap("    @Test\n    fun t() { assertTrue(true) }"));
        let b = kt(&kt_wrap("    @Test\n    fun t() { assertTrue(false) }"));
        assert_ne!(a[0].content, b[0].content);
    }

    // [FR-062]
    #[test]
    fn kotlinの注釈だけの変更はcontentを変えない() {
        let a = kt(&kt_wrap("    @Test\n    fun t() { assertTrue(true) }"));
        let b = kt(&kt_wrap(
            "    @Test\n    @Tag(\"a\")\n    fun t() { assertTrue(true) }",
        ));
        assert_eq!(a[0].content, b[0].content);
    }

    // [FR-014]
    #[test]
    fn kotlinは構文エラーのソースでエラーを返す() {
        let source = "class A { fun x() {} }\n";
        assert!(extract_kotlin_tests("tests/A.kt", source).is_err());
    }

    // [FR-060]
    #[test]
    fn kotlinのマーカーは元のソースから取り非asciiの角括弧は偽マーカーにならない() {
        let source = kt_wrap(
            "    // [正常系]\n    // [FR-001]\n    @Test\n    fun t() { }\n\n    // [仮] メモ\n    @Test\n    fun u() { }\n\n    // [FR\u{2010}001]\n    @Test\n    fun v() { }",
        );
        let tests = kt(&source);
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "u").markers.is_empty());
        assert!(find(&tests, "v").markers.is_empty());
    }

    // [FR-060]
    #[test]
    fn kotlinはdocコメントを挟んでも手前のマーカーが有効になる() {
        let source = kt_wrap("    // [FR-001]\n    /** KDoc */\n    @Test\n    fun b() { }");
        assert_eq!(kt(&source)[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-060]
    #[test]
    fn javaはdocコメントを挟んでも手前のマーカーが有効になる() {
        let source = jv_wrap("    // [FR-001]\n    /** Javadoc */\n    @Test\n    void b() { }");
        assert_eq!(jv(&source)[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-060]
    #[test]
    fn javaは注釈同士の間のコメントをマーカーにせずcontentにも含めない() {
        let source =
            jv_wrap("    @Test\n    // [FR-001]\n    @Tag(\"a\")\n    // 補足\n    void t() { }");
        let t = &jv(&source)[0];
        assert!(t.markers.is_empty());
        assert!(!t.content.contains("FR-001"));
        assert!(!t.content.contains("補足"));
    }

    // [FR-062]
    #[test]
    fn kotlinは注釈の間と直後のコメントをcontentに含めない() {
        let source = kt_wrap(
            "    @Test\n    // 間のメモ\n    @Tag(\"a\")\n    // 直後のメモ\n    fun t() { }",
        );
        let t = &kt(&source)[0];
        assert!(!t.content.contains("メモ"), "{:?}", t.content);
        assert!(t.content.starts_with("fun t()"), "{:?}", t.content);
    }

    // [FR-060]
    #[test]
    fn kotlinは注釈同士の間のコメントをマーカーにしない() {
        let source = kt_wrap("    @Test\n    // [FR-001]\n    @Tag(\"a\")\n    fun t() { }");
        assert!(kt(&source)[0].markers.is_empty());
    }

    // [FR-061]
    #[test]
    fn kotlinはdata_classとinterfaceの直下のtestも辿る() {
        let source = "data class D(val a: Int) {\n    @Test\n    fun a() { }\n}\n\ninterface I {\n    @Test\n    fun b() { }\n}\n";
        let tests = kt(source);
        assert_eq!(find(&tests, "a").id, "tests/AuthTest:D:a");
        assert_eq!(find(&tests, "b").id, "tests/AuthTest:I:b");
    }

    // [FR-061]
    #[test]
    fn kotlinは制御文字や空を含む名前のtestを無視する() {
        let source =
            kt_wrap("    @Test\n    fun `tab\there`() { }\n\n    @Test\n    fun `ok name`() { }");
        let tests = kt(&source);
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].name, "ok name");
    }

    // [FR-063]
    #[test]
    fn kotlinは注釈2つで基底型なしのクラス内の注釈付きinner_classでもtestを取れる() {
        let source = "@A\n@C\nclass P {\n    @Nested\n    inner class X {\n        // [FR-001]\n        @Test\n        fun t() { }\n    }\n}\n";
        let tests = kt(source);
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].id, "tests/AuthTest:P/X:t");
        assert_eq!(tests[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-063]
    #[test]
    fn kotlinはnestedを含む複数注釈クラスでも非asciiの名前を元のまま取る() {
        let source = "@A\n@C\nclass P {\n    @Nested\n    inner class 一覧取得 {\n        @Test\n        fun `取得 する`() { }\n    }\n}\n";
        let tests = kt(source);
        assert_eq!(tests[0].id, "tests/AuthTest:P/一覧取得:取得 する");
    }

    // [FR-063]
    #[test]
    fn kotlinは宣言以外のトップレベル構造やキーワードを含む文字列でもok空になる() {
        for source in [
            "package a\n// helper for class X\nval x = 1\n",
            "package a\nval fixture = object : Runnable {\n    override fun run() {}\n}\n",
            "package a\nval f = fun() { }\n",
            "package a\nconst val NAME = \"class\"\n",
        ] {
            let tests = kt(source);
            assert!(tests.is_empty(), "{source:?}: {tests:?}");
        }
    }

    // [FR-063]
    #[test]
    fn kotlinは文法の不具合で宣言が認識されなかった場合に構文エラーを返す() {
        let source = "@A\n@C\nclass P {\n    @Nested\n    inner class X {\n    }\n}\n";
        assert!(kotlin_extract("tests/P.kt", source, true).is_ok());
        assert!(kotlin_extract("tests/P.kt", source, false).is_err());
    }

    // [FR-063]
    #[test]
    fn kotlinはshebangやimportやtypealiasなど宣言以外の通常の構造でok空になる() {
        let source = "#!/usr/bin/env kotlin\n@file:JvmName(\"X\")\npackage a\n\nimport b.C\n\ntypealias N = String\nval x = 1\nval y: Int\n    get() = 2\n";
        assert!(kt(source).is_empty());
    }

    // ---- コメント隣接判定（FR-016 / FR-064） ----

    // [FR-016]
    #[test]
    fn rustはdocコメントを挟んでもマーカーを取る() {
        let tests = rs("// [FR-001]\n/// doc\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
    }

    // [FR-016]
    #[test]
    fn rustの外側docコメントはマーカーにならない() {
        let tests = rs("/// [FR-001]\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-016]
    #[test]
    fn rustの内側docコメントはマーカーにならない() {
        let tests = rs("mod m {\n//! [FR-001]\n#[test]\nfn t() {}\n}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-016]
    #[test]
    fn rustはブロックdocコメントを挟んでもマーカーを取る() {
        let tests = rs("// [FR-001]\n/** doc */\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
    }

    // [FR-016]
    #[test]
    fn rustのスラッシュ4つのコメントは通常コメントだが余分な記号が本文に残りマーカーにならない() {
        let tests = rs("//// [FR-001]\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-016]
    #[test]
    fn rustはdocコメントの後に空行があるとマーカーを取らない() {
        let tests = rs("// [FR-001]\n/// doc\n\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-016]
    #[test]
    fn rustはcrlfでもdocコメントを挟んでマーカーを取る() {
        let tests = rs("// [FR-001]\r\n/// doc\r\n#[test]\r\nfn t() {}\r\n", &[]);
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
    }

    // [FR-064]
    #[test]
    fn goは前の宣言の末尾行コメントを次のtestのマーカーにしない() {
        let source =
            "package a\n\nfunc TestA(t *testing.T) {} // [FR-001]\nfunc TestB(t *testing.T) {}\n";
        let tests = extract_go_tests("a/x_test.go", source).unwrap();
        assert!(find(&tests, "TestB").markers.is_empty());
    }

    // [FR-064]
    #[test]
    fn rustは前の宣言の末尾行コメントを次のtestのマーカーにしない() {
        let tests = rs("fn x() {} // [FR-001]\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-064]
    #[test]
    fn csharpは前の宣言の末尾行コメントを次のtestのマーカーにしない() {
        let source =
            cs_wrap("    public void X() {} // [FR-001]\n    [Fact]\n    public void T() {}");
        let tests = cs(&source);
        assert!(find(&tests, "T").markers.is_empty());
    }

    // [FR-064]
    #[test]
    fn typescriptは前の文の末尾行コメントを次のtestのマーカーにしない() {
        let tests = ts("it(\"a\", () => {}); // [FR-001]\nit(\"b\", () => {});\n");
        assert!(find(&tests, "b").markers.is_empty());
    }

    // [FR-064]
    #[test]
    fn javaは前の宣言の末尾行コメントを次のtestのマーカーにしない() {
        let source = jv_wrap("    void x() {} // [FR-001]\n    @Test\n    void t() {}");
        let tests = jv(&source);
        assert!(find(&tests, "t").markers.is_empty());
    }

    // [FR-064]
    #[test]
    fn kotlinは非ascii識別子の後の末尾行コメントを次のtestのマーカーにしない() {
        let source = kt_wrap("    fun `日本語`() {} // [FR-001]\n    @Test\n    fun t() {}");
        let tests = kt(&source);
        assert!(find(&tests, "t").markers.is_empty());
    }

    // [FR-064]
    #[test]
    fn rustはインデント付きの行頭コメントをマーカーにする() {
        let tests = rs(
            "mod m {\n    // [FR-001]\n    #[test]\n    fn t() {}\n}\n",
            &[],
        );
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
    }

    // [FR-064]
    #[test]
    fn typescriptは同じ行に複数のコメントがあると2つ目以降は行頭でないので打ち切る() {
        let tests = ts("/* [FR-001] */ // note\nit(\"b\", () => {});\n");
        assert!(find(&tests, "b").markers.is_empty());
    }

    // [FR-064]
    #[test]
    fn rustはbomの直後の1行目マーカーを取る() {
        let tests = rs("\u{feff}// [FR-001]\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, vec!["FR-001".to_string()]);
    }

    // [FR-064]
    #[test]
    fn goはbomの直後の1行目マーカーを取る() {
        let source = "\u{feff}// [FR-001]\nfunc TestA(t *testing.T) {}\n";
        let tests = extract_go_tests("a/x_test.go", source).unwrap();
        assert_eq!(find(&tests, "TestA").markers, vec!["FR-001".to_string()]);
    }

    // [FR-016]
    #[test]
    fn rustの内側ブロックdocコメントはマーカーにならない() {
        let tests = rs("mod m {\n/*! [FR-001] */\n#[test]\nfn t() {}\n}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-016]
    #[test]
    fn rustのスラッシュ星3つのコメントは通常コメントだが余分な記号が本文に残りマーカーにならない() {
        let tests = rs("/*** [FR-001] */\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // [FR-016]
    #[test]
    fn rustの外側ブロックdocコメントはマーカーにならない() {
        let tests = rs("/** [FR-001] */\n#[test]\nfn t() {}\n", &[]);
        assert_eq!(find(&tests, "t").markers, Vec::<String>::new());
    }

    // ---- マーカーの厳格化（FR-121） ----

    /// (コメント, 期待するマーカー)。コメントの直後に test が続く。
    fn marker_cases() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![
            ("// [FR-001]", vec!["FR-001"]),
            ("//   [FR-001]   ", vec!["FR-001"]),
            ("// [FR-001] [FR-002]", vec!["FR-001", "FR-002"]),
            ("// [FR-001]\n// [FR-002]", vec!["FR-001", "FR-002"]),
            ("//// [FR-001]", vec![]),
            ("/*** [FR-001] */", vec![]),
            ("// [FR-001] 説明", vec![]),
            ("// 説明 [FR-001]", vec![]),
            ("// 説明 [0] 説明", vec![]),
            ("// [FR-001] [正常系]", vec![]),
            ("// [FR-001][FR-002]", vec![]),
            ("//", vec![]),
            ("/* [FR-001] */", vec!["FR-001"]),
            ("/*\n * [FR-001]\n */", vec!["FR-001"]),
            (
                "/*\n * [FR-001]\n * [FR-002]\n */",
                vec!["FR-001", "FR-002"],
            ),
            ("/* [FR-001] [FR-002] */", vec!["FR-001", "FR-002"]),
            ("/*\n * [FR-001] 説明\n */", vec![]),
            ("/* 説明 [0] */", vec![]),
        ]
    }

    fn assert_marker_cases(markers_of: impl Fn(&str) -> Vec<String>) {
        for (comment, expected) in marker_cases() {
            assert_eq!(markers_of(comment), expected, "comment: {comment:?}");
        }
    }

    // [FR-121]
    #[test]
    fn goはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            let source = format!("{c}\nfunc TestA(t *testing.T) {{}}\n");
            find(&extract_go_tests("a/x_test.go", &source).unwrap(), "TestA")
                .markers
                .clone()
        });
    }

    // [FR-121]
    #[test]
    fn rustはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            rs(&format!("{c}\n#[test]\nfn t() {{}}\n"), &[])[0]
                .markers
                .clone()
        });
    }

    // [FR-121]
    #[test]
    fn csharpはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            cs(&cs_wrap(&format!("{c}\n[Fact]\npublic void T() {{ }}")))[0]
                .markers
                .clone()
        });
    }

    // [FR-121]
    #[test]
    fn javaはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            jv(&jv_wrap(&format!("{c}\n@Test\nvoid t() {{ }}")))[0]
                .markers
                .clone()
        });
    }

    // [FR-121]
    #[test]
    fn kotlinはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            kt(&kt_wrap(&format!("{c}\n@Test\nfun t() {{ }}")))[0]
                .markers
                .clone()
        });
    }

    // [FR-121]
    #[test]
    fn typescriptはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            ts(&format!("{c}\nit(\"t\", () => {{}});\n"))[0]
                .markers
                .clone()
        });
    }

    // [FR-121]
    #[test]
    fn dartはコメントが単一トークンだけのときマーカーにする() {
        assert_marker_cases(|c| {
            dt(&format!("void main() {{\n{c}\ntest('t', () {{}});\n}}\n"))[0]
                .markers
                .clone()
        });
    }
    // [FR-121]
    #[test]
    fn csharpの三連スラッシュコメントも単一トークンだけならマーカーにする() {
        let tests = cs(&cs_wrap("/// [FR-001]\n[Fact]\npublic void T() { }"));
        assert_eq!(tests[0].markers, vec!["FR-001".to_string()]);
        let tests = cs(&cs_wrap("/// [FR-001] 説明\n[Fact]\npublic void T() { }"));
        assert!(tests[0].markers.is_empty());
    }

    // ---- Dart / Flutter ----

    fn dt(source: &str) -> Vec<ExtractedTest> {
        extract_dart_tests("mobile/test/a_test.dart", source).unwrap()
    }

    // [FR-122]
    #[test]
    fn dartはtest_dartで終わるパスだけをテストファイルとして扱う() {
        assert!(is_dart_test_file("test/a_test.dart"));
        assert!(is_dart_test_file("a_test.dart"));
        assert!(!is_dart_test_file("test/a.dart"));
        assert!(!is_dart_test_file("test/a_test.dart.bak"));
        assert!(!is_dart_test_file("test/a_test.rs"));
        assert!(!is_dart_test_file("test/atest.dart"));
    }

    // [FR-123]
    #[test]
    fn dartはmain直下のtestとtestwidgetsを検出する() {
        let tests =
            dt("void main() {\n  test('a', () {});\n  testWidgets('b', (tester) async {});\n}\n");
        assert_eq!(tests.len(), 2);
        find(&tests, "a");
        find(&tests, "b");
    }

    // [FR-123]
    #[test]
    fn dartはgroupの入れ子を辿る() {
        let tests = dt("void main() {\n  group('outer', () {\n    group('inner', () {\n      test('t', () {});\n    });\n    test('u', () {});\n  });\n}\n");
        assert_eq!(tests.len(), 2);
        assert_eq!(find(&tests, "t").id, "mobile/test/a_test:outer/inner:t");
        assert_eq!(find(&tests, "u").id, "mobile/test/a_test:outer:u");
    }

    // [FR-125]
    #[test]
    fn dartのtest識別子はgroupの外なら空のgroupパスになる() {
        let tests = dt("void main() { test('t', () {}); }\n");
        assert_eq!(tests[0].id, "mobile/test/a_test::t");
    }

    // [FR-123]
    #[test]
    fn dartはfuture_void_main_asyncでも検出する() {
        let tests = dt("Future<void> main() async {\n  test('a', () {});\n}\n");
        assert_eq!(tests.len(), 1);
        let tests = dt("main() { test('a', () {}); }\n");
        assert_eq!(tests.len(), 1);
    }

    // [FR-123]
    #[test]
    fn dartはasyncコールバックとアロー本体のtestも検出する() {
        let tests = dt("void main() {\n  test('a', () async {});\n  test('b', () => expect(1, 1));\n  group('g', () async {\n    test('c', () {});\n  });\n}\n");
        assert_eq!(tests.len(), 3);
        assert_eq!(find(&tests, "c").id, "mobile/test/a_test:g:c");
    }

    // [FR-123]
    #[test]
    fn dartのgroupはアロー本体だと中へ降りない() {
        let tests = dt("void main() {\n  group('g', () => test('t', () {}));\n}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-123]
    #[test]
    fn dartは名前付き引数付きでも1件のtestとして扱う() {
        let tests = dt("void main() {\n  test('a', () {}, skip: true, timeout: Timeout.none);\n  group('g', () {\n    test('b', () {});\n  }, skip: 'x');\n}\n");
        assert_eq!(tests.len(), 2);
        assert_eq!(find(&tests, "b").id, "mobile/test/a_test:g:b");
    }

    // [FR-125]
    #[test]
    fn dartはraw文字列を引用符の内側のソース表記のまま名前にする() {
        let tests = dt("void main() {\n  test(r'a\\n$b', () {});\n  test(r\"c\\d\", () {});\n}\n");
        assert_eq!(tests.len(), 2);
        find(&tests, "a\\n$b");
        find(&tests, "c\\d");
    }

    // [FR-125]
    #[test]
    fn dartは隣接文字列連結を各部分の内側で連結して名前にする() {
        let tests = dt("void main() {\n  test('abc '\n      \"def \"\n      r'g\\h', () {});\n}\n");
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "abc def g\\h");
    }

    // [FR-125]
    #[test]
    fn dartはエスケープを解釈せずソース表記のまま名前にする() {
        let tests = dt("void main() {\n  test('it\\'s \\$x', () {});\n}\n");
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "it\\'s \\$x");
    }

    // [FR-125]
    #[test]
    fn dartは補間を含む名前のtestとgroupを無視する() {
        let tests = dt("void main() {\n  test('a $x', () {});\n  test('b ${y}', () {});\n  test('c' 'd$z', () {});\n  group('g$x', () {\n    test('t', () {});\n  });\n}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-125]
    #[test]
    fn dartは三重引用符の名前を無視する() {
        let tests = dt("void main() {\n  test('''a''', () {});\n  test(\"\"\"b\"\"\", () {});\n  test(r'''c''', () {});\n  test('d' '''e''', () {});\n}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-125]
    #[test]
    fn dartは無効な名前のgroup配下を走査しない() {
        for name in ["''", "'''x'''", "'a\u{2028}b'", "'a\u{1}b'"] {
            let source = format!(
                "void main() {{\n  group({name}, () {{\n    test('t', () {{}});\n  }});\n}}\n"
            );
            let tests = dt(&source);
            assert!(tests.is_empty(), "{name:?}: {tests:?}");
        }
    }

    // [FR-125]
    #[test]
    fn dartは行区切り文字や制御文字を含むtest名を無視する() {
        let tests = dt("void main() {\n  test('a\u{2028}b', () {});\n  test('c\u{1}d', () {});\n  test('ok', () {});\n}\n");
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].name, "ok");
    }

    // [FR-125]
    #[test]
    fn dartは空文字名を無視する() {
        let tests = dt(
            "void main() {\n  test('', () {});\n  test('' '', () {});\n  test('ok', () {});\n}\n",
        );
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "ok");
    }

    // [FR-125]
    #[test]
    fn dartは第1引数が文字列リテラルでない呼び出しを無視する() {
        let tests = dt("void main() {\n  test(name, () {});\n  group(name, () {\n    test('t', () {});\n  });\n  test();\n  test(skip: true);\n}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-123]
    #[test]
    fn dartは修飾付きやメソッドチェーンやawait付きの呼び出しを無視する() {
        let tests = dt("import 'package:test/test.dart' as t;\nvoid main() {\n  t.test('a', () {});\n  t.group('g', () {\n    test('b', () {});\n  });\n  test('c', () {}).foo();\n  await test('d', () {});\n  test('e', () {});\n}\n");
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].name, "e");
    }

    // [FR-123]
    #[test]
    fn dartはtestとgroupとtestwidgets以外の呼び出しを無視する() {
        let tests =
            dt("void main() {\n  setUp(() {});\n  it('a', () {});\n  Test('b', () {});\n}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-123]
    #[test]
    fn dartはforとifと関数とtest本体の中のtestを無視する() {
        let source = "void helper() {\n  test('h', () {});\n}\nvoid main() {\n  for (final x in [1]) {\n    test('f', () {});\n  }\n  if (true) {\n    test('i', () {});\n  }\n  test('outer', () {\n    test('inner', () {});\n  });\n  void local() {\n    test('l', () {});\n  }\n  group('g', () {\n    for (var i = 0; i < 2; i++) {\n      test('gf', () {});\n    }\n    test('ok', () {});\n  });\n}\n";
        let tests = dt(source);
        let mut names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["ok", "outer"], "{tests:?}");
    }

    // [FR-123]
    #[test]
    fn dartはmain以外のトップレベル関数を入口にしない() {
        let tests =
            dt("void other() {\n  test('a', () {});\n}\nvoid mainx() {\n  test('b', () {});\n}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-123]
    #[test]
    fn dartはmainが無いファイルを0件にする() {
        assert!(dt("int x = 1;\n").is_empty());
        assert!(dt("").is_empty());
    }

    // [FR-124]
    #[test]
    fn dartは直前に隣接するコメントをマーカーとして抽出し空行を挟むと無視する() {
        let source = "void main() {\n  // [FR-001]\n  test('a', () {});\n\n  // [FR-002]\n\n  test('b', () {});\n  /* [FR-003] */\n  test('c', () {});\n}\n";
        let tests = dt(source);
        assert_eq!(find(&tests, "a").markers, vec!["FR-001".to_string()]);
        assert!(find(&tests, "b").markers.is_empty());
        assert_eq!(find(&tests, "c").markers, vec!["FR-003".to_string()]);
    }

    // [FR-124]
    #[test]
    fn dartはgroup内のtestでも直前のコメントをマーカーにする() {
        let tests = dt(
            "void main() {\n  group('g', () {\n    // [FR-001]\n    test('t', () {});\n  });\n}\n",
        );
        assert_eq!(tests[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-124]
    #[test]
    fn dartのドキュメントコメントはマーカーにならない() {
        let source = "void main() {\n  /// [FR-001]\n  test('a', () {});\n  /** [FR-002] */\n  test('b', () {});\n  //// [FR-003]\n  test('c', () {});\n}\n";
        let tests = dt(source);
        for n in ["a", "b", "c"] {
            assert!(find(&tests, n).markers.is_empty(), "{n}");
        }
    }

    // [FR-124]
    #[test]
    fn dartはマーカーとtestの間にdocコメントが挟まってもマーカーを有効にする() {
        let source = "void main() {\n  // [FR-001]\n  /// 説明 [Foo]\n  test('a', () {});\n  // [FR-002]\n  /** doc */\n  test('b', () {});\n}\n";
        let tests = dt(source);
        assert_eq!(find(&tests, "a").markers, vec!["FR-001".to_string()]);
        assert_eq!(find(&tests, "b").markers, vec!["FR-002".to_string()]);
    }

    // [FR-124]
    #[test]
    fn dartは前の文の末尾行コメントを次のtestのマーカーにしない() {
        let tests = dt("void main() {\n  test('a', () {}); // [FR-001]\n  test('b', () {});\n}\n");
        assert!(find(&tests, "b").markers.is_empty());
    }

    // [FR-123]
    #[test]
    fn dartのcontentは呼び出し文全体でマーカーコメントを含まない() {
        let tests = dt("void main() {\n  // [FR-001]\n  test('a', () { expect(1, 1); });\n}\n");
        assert_eq!(tests[0].content, "test('a', () { expect(1, 1); });");
    }

    // [FR-123]
    #[test]
    fn dartの本体変更はcontentに反映される() {
        let a = dt("void main() { test('a', () { expect(1, 1); }); }\n");
        let b = dt("void main() { test('a', () { expect(1, 2); }); }\n");
        assert_ne!(a[0].content, b[0].content);
    }

    // [FR-123]
    #[test]
    fn dartは同名のtestをそのまま複数返す() {
        let tests = dt("void main() {\n  test('a', () {});\n  test('a', () {});\n}\n");
        assert_eq!(tests.len(), 2);
        assert_eq!(tests[0].id, tests[1].id);
    }

    // [FR-014]
    #[test]
    fn dartは構文エラーのソースでエラーを返す() {
        let source = "void main() {\n  test('a', () {\n}\n";
        assert!(extract_dart_tests("mobile/test/a_test.dart", source).is_err());
    }

    // ---- Kotlin の位置引数付き注釈（FR-063） ----

    const KOTLIN_KDOC_FIXTURE: &str = include_str!("../fixtures/kotlin_kdoc_annotation.kt");
    const KOTLIN_SUPPORT_FIXTURE: &str = include_str!("../fixtures/kotlin_annotation_support.kt");

    // [FR-063]
    #[test]
    fn kotlinは直前の宣言とコメントの後の位置引数付き注釈でも解析できる() {
        let tests = kt("fun g() {}\n// a\n@S(\"L\")\nfun f() {}\n");
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-063]
    #[test]
    fn kotlinは非asciiのkdocと位置引数付き注釈の組み合わせでも解析できる() {
        let tests = kt(KOTLIN_KDOC_FIXTURE);
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-063]
    #[test]
    fn kotlinは位置引数付き注釈を持つサポートコードのファイルを解析できる() {
        let tests = kt(KOTLIN_SUPPORT_FIXTURE);
        assert!(tests.is_empty(), "{tests:?}");
    }

    // [FR-063]
    #[test]
    fn kotlinは位置引数付き注釈の後ろにtestが続いてもtestを取る() {
        let tests = kt("fun g(){}\n// a\n@DisplayName(\"x\")\n@Test\nfun f(){}\n");
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].name, "f");
    }

    // [FR-063]
    #[test]
    fn kotlinは位置引数付き注釈の後ろのtestのマーカーも取る() {
        let tests = kt("fun g(){}\n// a\n// [FR-001]\n@DisplayName(\"x\")\n@Test\nfun f(){}\n");
        assert_eq!(tests.len(), 1, "{tests:?}");
        assert_eq!(tests[0].markers, vec!["FR-001".to_string()]);
    }

    // [FR-014]
    #[test]
    fn kotlinは位置引数付き注釈のある本当の構文エラーを引き続きエラーにする() {
        let source = "fun g() {}\n// a\n@S(\"L\")\nfun f( {\n";
        assert!(extract_kotlin_tests("tests/A.kt", source).is_err());
    }

    // [FR-063]
    #[test]
    fn kotlinは位置引数付き注釈が複数あってもtestを取る() {
        let tests = kt("fun g(){}\n// a\n@A(\"x\")\n@B(\"y\")\n@Test\nfun f(){}\n");
        assert_eq!(tests.len(), 1, "{tests:?}");
    }
}
