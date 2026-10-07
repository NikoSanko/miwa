mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CFG: &str = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\nindex:\n  - docs\n";

const CSP: &str = r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self' https:; style-src 'unsafe-inline'">"#;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

/// Repository root named `proj` with the default config (index: docs).
fn fixture() -> Fixture {
    fixture_with_config(CFG)
}

fn fixture_with_config(config: &str) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    common::write(&root, ".miwa/config.yaml", config);
    Fixture { _tmp: tmp, root }
}

fn cfg_with(extra: &str) -> String {
    format!("{CFG}{extra}")
}

fn doc(id: &str, title: &str, body: &str) -> String {
    format!("---\ntype: Specification\nid: {id}\ntitle: {title}\n---\n{body}")
}

fn req(id: &str, title: &str, body: &str) -> String {
    format!("---\ntype: Requirement\nid: {id}\ntitle: {title}\n---\n{body}")
}

fn reference(title: &str, body: &str) -> String {
    format!("---\ntype: Reference\ntitle: {title}\n---\n{body}")
}

/// A root index page with a title and description.
fn index_md() -> &'static str {
    "---\ntitle: 文書\ndescription: 文書の一覧\n---\n- [Spec](spec.md) — 説明\n"
}

struct Out {
    code: i32,
    stderr: String,
}

impl Out {
    fn warnings(&self) -> Vec<&str> {
        self.stderr
            .lines()
            .filter(|l| l.starts_with("warning: "))
            .collect()
    }
}

fn site(root: &Path, args: &[&str]) -> Out {
    let mut full = vec!["site"];
    full.extend_from_slice(args);
    let o = common::run(root, &full).output().unwrap();
    Out {
        code: o.status.code().unwrap(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn site_ok(root: &Path) -> Out {
    let o = site(root, &[]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    o
}

fn page(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join("_site").join(rel))
        .unwrap_or_else(|e| panic!("missing _site/{rel}: {e}"))
}

fn exists(root: &Path, rel: &str) -> bool {
    root.join("_site").join(rel).exists()
}

/// Removes whitespace between tags so structural checks do not depend on line breaks.
fn squash(html: &str) -> String {
    let mut out = String::new();
    let mut pending_ws = String::new();
    for ch in html.chars() {
        if ch.is_whitespace() {
            pending_ws.push(ch);
            continue;
        }
        if !pending_ws.is_empty() {
            if !(ch == '<' && out.ends_with('>')) {
                out.push_str(&pending_ws);
            }
            pending_ws.clear();
        }
        out.push(ch);
    }
    out
}

fn files_under(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, acc: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, acc);
            } else {
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                acc.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut acc = BTreeMap::new();
    walk(dir, dir, &mut acc);
    acc
}

fn site_files(root: &Path) -> Vec<String> {
    files_under(&root.join("_site")).into_keys().collect()
}

/// Ids of all `<h{level} ...>` headings in document order (`None` when without `id`).
fn heading_ids(html: &str, level: u8) -> Vec<Option<String>> {
    let open = format!("<h{level}");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        rest = &rest[i + open.len()..];
        match rest.chars().next() {
            Some('>') | Some(' ') | Some('\n') | Some('\t') => {}
            _ => continue,
        }
        let end = rest.find('>').unwrap();
        out.push(attr(&rest[..end], "id"));
    }
    out
}

/// Looks for `<h{level} attrs>inner</h{level}>`; `Some(id)` when found.
fn heading_id(html: &str, level: u8, inner: &str) -> Option<Option<String>> {
    let open = format!("<h{level}");
    let close = format!("</h{level}>");
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        rest = &rest[i + open.len()..];
        match rest.chars().next() {
            Some('>') | Some(' ') | Some('\n') | Some('\t') => {}
            _ => continue,
        }
        let end = rest.find('>').unwrap();
        let attrs = &rest[..end];
        let after = &rest[end + 1..];
        if after.starts_with(inner) && after[inner.len()..].starts_with(&close) {
            return Some(attr(attrs, "id"));
        }
    }
    None
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = attrs.find(&key)? + key.len();
    let len = attrs[start..].find('"')?;
    Some(attrs[start..start + len].to_string())
}

/// The `content` of `<meta name="description" ...>` regardless of attribute order.
fn meta_description(html: &str) -> Option<String> {
    let mut rest = html;
    while let Some(i) = rest.find("<meta") {
        rest = &rest[i + 5..];
        let end = rest.find('>')?;
        let tag = &rest[..end];
        if attr(tag, "name").as_deref() == Some("description") {
            return attr(tag, "content");
        }
    }
    None
}

/// Number of `<a ` / `<a>` anchors in the page.
fn anchor_count(html: &str) -> usize {
    html.matches("<a ").count() + html.matches("<a>").count()
}

fn pos(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in:\n{haystack}"))
}

/// Standard small fixture: index page, one doc, one req.
fn basic() -> Fixture {
    let f = fixture();
    common::write(&f.root, "docs/index.md", index_md());
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "本文です。\n"),
    );
    common::write(
        &f.root,
        "req/fr1.md",
        &req("FR-001", "要件1", "要件本文。\n"),
    );
    f
}

// ---------------------------------------------------------------- FR-089

// [FR-089]
#[test]
fn 出力先の既定は_siteで存在しなければ作られる() {
    let f = basic();
    assert!(!f.root.join("_site").exists());
    site_ok(&f.root);
    assert!(f.root.join("_site/index.html").is_file());
}

// [FR-089]
#[test]
fn out指定の出力先が存在しなければ作られる() {
    let f = basic();
    let o = site(&f.root, &["--out", "build/out"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(f.root.join("build/out/index.html").is_file());
    assert!(!f.root.join("_site").exists());
}

// [FR-089]
#[test]
fn 存在して空の出力先はそのまま使われる() {
    let f = basic();
    std::fs::create_dir_all(f.root.join("out")).unwrap();
    let o = site(&f.root, &["--out", "out"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(f.root.join("out/index.html").is_file());
}

// [FR-089]
#[test]
fn 出力先にファイルが1つでもあるとエラーで何も書かない() {
    let f = basic();
    common::write(&f.root, "out/keep.txt", "KEEP");
    let o = site(&f.root, &["--out", "out"]);
    assert_eq!(o.code, 1);
    assert!(!o.stderr.trim().is_empty());
    let files = files_under(&f.root.join("out"));
    assert_eq!(files.keys().collect::<Vec<_>>(), vec!["keep.txt"]);
    assert_eq!(files["keep.txt"], b"KEEP");
}

// [FR-089]
#[test]
fn 出力先が空のサブディレクトリだけを持つ場合もエラーで何も書かない() {
    let f = basic();
    std::fs::create_dir_all(f.root.join("out/empty-sub")).unwrap();
    let o = site(&f.root, &["--out", "out"]);
    assert_eq!(o.code, 1);
    assert!(!f.root.join("out/index.html").exists());
    assert!(f.root.join("out/empty-sub").is_dir());
}

// [FR-089]
#[test]
fn 出力先がファイルのときはエラーで何も書かず中身も変わらない() {
    let f = basic();
    common::write(&f.root, "out", "I AM A FILE");
    let o = site(&f.root, &["--out", "out"]);
    assert_eq!(o.code, 1);
    assert_eq!(
        std::fs::read_to_string(f.root.join("out")).unwrap(),
        "I AM A FILE"
    );
}

// [FR-089]
#[test]
fn out_ドットのようなリポジトリ自身の指定はエラーで既存ファイルを消さない() {
    let f = basic();
    let before = files_under(&f.root);
    let o = site(&f.root, &["--out", "."]);
    assert_eq!(o.code, 1);
    assert_eq!(files_under(&f.root), before);
}

// ---------------------------------------------------------------- FR-090

// [FR-090]
#[test]
fn indexが未設定ならメッセージを出して失敗し何も書かない() {
    let f = fixture_with_config(common::config_yaml_without_index());
    common::write(&f.root, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    let o = site(&f.root, &[]);
    assert_eq!(o.code, 1);
    assert!(
        o.stderr
            .contains("index is not configured in .miwa/config.yaml"),
        "{}",
        o.stderr
    );
    assert!(!f.root.join("_site").exists());
}

// [FR-090]
#[test]
fn indexが未設定で出力先を指定しても何も作られない() {
    let f = fixture_with_config(common::config_yaml_without_index());
    let o = site(&f.root, &["--out", "o"]);
    assert_eq!(o.code, 1);
    assert!(!f.root.join("o").exists());
}

// [FR-090]
#[test]
fn ロックが無くても壊れていても成功しロックを作らない() {
    let f = basic();
    assert!(!f.root.join(".miwa/lock.json").exists());
    site_ok(&f.root);
    assert!(!f.root.join(".miwa/lock.json").exists());

    common::write(&f.root, ".miwa/lock.json", "this is not json");
    let o = site(&f.root, &["--out", "second"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(
        std::fs::read_to_string(f.root.join(".miwa/lock.json")).unwrap(),
        "this is not json"
    );
}

// [FR-090]
#[test]
fn 同じ入力から2回生成するとファイル集合も内容もバイト一致する() {
    let f = basic();
    common::write(
        &f.root,
        "docs/img.md",
        &reference("画像", "![a](pic/a.png)\n\n[l](spec.md) [m](missing.md)\n"),
    );
    std::fs::create_dir_all(f.root.join("docs/pic")).unwrap();
    std::fs::write(
        f.root.join("docs/pic/a.png"),
        [0x89, b'P', b'N', b'G', 1, 2, 3],
    )
    .unwrap();
    let a = site(&f.root, &["--out", "o1"]);
    let b = site(&f.root, &["--out", "o2"]);
    assert_eq!(a.code, 0, "{}", a.stderr);
    assert_eq!(b.code, 0, "{}", b.stderr);
    let fa = files_under(&f.root.join("o1"));
    let fb = files_under(&f.root.join("o2"));
    assert!(fa.len() >= 5, "{:?}", fa.keys());
    assert_eq!(fa, fb);
    assert_eq!(a.stderr, b.stderr);
}

// [FR-090]
#[test]
fn 公開されないリンクがあっても終了コード0で警告だけ出す() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[x](missing.md) [y](javascript:void)\n"),
    );
    let o = site_ok(&f.root);
    assert_eq!(o.warnings().len(), 2, "{}", o.stderr);
}

// ---------------------------------------------------------------- FR-091

// [FR-091]
#[test]
fn 公開されるページはdocとreqと索引対象文書と索引ページの和集合に限る() {
    let f = fixture_with_config(&cfg_with("exclude:\n  - drafts\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    common::write(r, "docs/ref.md", &reference("参考", "x\n"));
    common::write(
        r,
        "docs/ref2.md",
        "---\ntype: Reference\nid: DUP\ntitle: R2\n---\nx\n",
    );
    common::write(
        r,
        "docs/ref3.md",
        "---\ntype: Reference\nid: DUP\ntitle: R3\n---\nx\n",
    );
    common::write(
        r,
        "docs/ref4.md",
        "---\ntype: Reference\nid: bad id!\ntitle: R4\n---\nx\n",
    );
    common::write(r, "docs/sub/index.md", "- [R5](r5.md)\n");
    common::write(r, "docs/sub/r5.md", &reference("R5", "x\n"));
    common::write(r, "req/fr1.md", &req("FR-001", "要件1", "x\n"));
    // not published
    common::write(r, "docs/untyped.md", "# 型なし\n");
    common::write(r, "docs/log.md", &reference("Log", "x\n"));
    common::write(r, "docs/drafts/d.md", &reference("D", "x\n"));
    common::write(r, "docs/node_modules/n.md", &reference("N", "x\n"));
    common::write(r, "docs/note.txt", "text\n");
    common::write(
        r,
        "docs/bad.md",
        "---\ntype: Specification\nid: [\n---\nx\n",
    );
    common::write(r, "docs/unterm.md", "---\ntype: Specification\nid: DOC-U\n");
    common::write(r, "req/untyped.md", "plain\n");
    common::write(r, "other/x.md", &reference("X", "x\n"));

    site_ok(r);
    assert_eq!(
        site_files(r),
        vec![
            "docs/index.html",
            "docs/ref.html",
            "docs/ref2.html",
            "docs/ref3.html",
            "docs/ref4.html",
            "docs/spec.html",
            "docs/sub/index.html",
            "docs/sub/r5.html",
            "index.html",
            "req/fr1.html",
        ]
    );
}

// [FR-091]
#[test]
fn indexの根の外のdocとreqはページになり根の外の型付き文書はならない() {
    let f = fixture_with_config(
        &cfg_with("").replace("index:\n  - docs\n", "index:\n  - docs/guide\n"),
    );
    let r = &f.root;
    common::write(r, "docs/guide/index.md", "- [G](g.md)\n");
    common::write(r, "docs/guide/g.md", &reference("G", "x\n"));
    common::write(r, "docs/other.md", &doc("DOC-O", "Other", "x\n"));
    common::write(r, "docs/loose.md", &reference("Loose", "x\n"));
    common::write(r, "req/fr1.md", &req("FR-001", "要件1", "x\n"));
    site_ok(r);
    assert_eq!(
        site_files(r),
        vec![
            "docs/guide/g.html",
            "docs/guide/index.html",
            "docs/other.html",
            "index.html",
            "req/fr1.html",
        ]
    );
}

// ---------------------------------------------------------------- FR-092

// [FR-092]
#[test]
fn 出力パスは元のパスの拡張子をhtmlに置き換えたものになる() {
    let f = fixture();
    let r = &f.root;
    common::write(r, "docs/index.md", "- [B](a/b.md)\n");
    common::write(r, "docs/a/b.md", &doc("DOC-B", "B", "x\n"));
    common::write(r, "req/x/fr1.md", &req("FR-001", "R", "x\n"));
    site_ok(r);
    assert_eq!(
        site_files(r),
        vec![
            "docs/a/b.html",
            "docs/index.html",
            "index.html",
            "req/x/fr1.html"
        ]
    );
}

// ---------------------------------------------------------------- FR-093

// [FR-093]
#[test]
fn トップページのタイトルと要約はsite設定から決まる() {
    let f = fixture_with_config(&cfg_with(
        "site:\n  title: \"○○ & <プロジェクト>\"\n  description: \"閲覧用 & <サイト>\"\n",
    ));
    common::write(&f.root, "docs/index.md", index_md());
    site_ok(&f.root);
    let top = page(&f.root, "index.html");
    assert!(
        top.contains("<title>○○ &amp; &lt;プロジェクト&gt;</title>"),
        "{top}"
    );
    assert!(
        heading_id(&top, 1, "○○ &amp; &lt;プロジェクト&gt;").is_some(),
        "{top}"
    );
    assert!(
        (meta_description(&top).as_deref() == Some("閲覧用 &amp; &lt;サイト&gt;")),
        "{top}"
    );
    let h1_end = pos(&top, "</h1>");
    let p = pos(
        &top,
        r#"<p class="description">閲覧用 &amp; &lt;サイト&gt;</p>"#,
    );
    assert!(h1_end < p);
}

// [FR-093]
#[test]
fn site設定が無いときタイトルは実行ディレクトリ名で要約は出ない() {
    let f = basic();
    site_ok(&f.root);
    let top = page(&f.root, "index.html");
    assert!(top.contains("<title>proj</title>"), "{top}");
    assert!(heading_id(&top, 1, "proj").is_some(), "{top}");
    assert!(meta_description(&top).is_none(), "{top}");
}

// [FR-093]
#[test]
fn site設定の要約だけを省略するとタイトルだけが使われる() {
    let f = fixture_with_config(&cfg_with("site:\n  title: だけ\n"));
    common::write(&f.root, "docs/index.md", index_md());
    site_ok(&f.root);
    let top = page(&f.root, "index.html");
    assert!(heading_id(&top, 1, "だけ").is_some(), "{top}");
    assert!(meta_description(&top).is_none(), "{top}");
}

// [FR-093]
#[test]
fn トップページはindex設定の根を順にタイトルと要約の一覧にする() {
    let f = fixture_with_config(
        &cfg_with("").replace("index:\n  - docs\n", "index:\n  - docs/b\n  - docs/a\n"),
    );
    let r = &f.root;
    common::write(
        r,
        "docs/a/index.md",
        "---\ntitle: エー\ndescription: 要約A\n---\n",
    );
    common::write(r, "docs/b/index.md", "---\ntitle: ビー\n---\n");
    common::write(r, "docs/a/x.md", &reference("X", "x\n"));
    common::write(r, "docs/b/y.md", &reference("Y", "x\n"));
    site_ok(r);
    let top = squash(&page(r, "index.html"));
    let b = pos(&top, r#"<li><a href="docs/b/index.html">ビー</a></li>"#);
    let a = pos(
        &top,
        r#"<li><a href="docs/a/index.html">エー</a> — 要約A</li>"#,
    );
    assert!(b < a, "{top}");
    assert!(top.contains("<ul><li>"), "{top}");
    assert!(top.contains("</li></ul>"), "{top}");
}

// [FR-093]
#[test]
fn トップページのエントリは根のindexmdのタイトルが不正ならディレクトリ名と要約なしになる() {
    let f = fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "---\ntitle: \"a\\nb\"\ndescription: |\n  行1\n  行2\n---\n",
    );
    common::write(r, "docs/x.md", &reference("X", "x\n"));
    site_ok(r);
    let top = squash(&page(r, "index.html"));
    assert!(
        top.contains(r#"<li><a href="docs/index.html">docs</a></li>"#),
        "{top}"
    );
}

// [FR-093]
#[test]
fn トップページのエントリは空のタイトルと空の要約を無いものとして扱う() {
    let f = fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "---\ntitle: \"  \"\ndescription: \"  \"\n---\n",
    );
    common::write(r, "docs/x.md", &reference("X", "x\n"));
    site_ok(r);
    let top = squash(&page(r, "index.html"));
    assert!(
        top.contains(r#"<li><a href="docs/index.html">docs</a></li>"#),
        "{top}"
    );
}

// [FR-093]
#[test]
fn トップページのエントリのタイトルと要約はエスケープされる() {
    let f = fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "---\ntitle: \"A & <b>\"\ndescription: \"x < y\"\n---\n",
    );
    common::write(r, "docs/x.md", &reference("X", "x\n"));
    site_ok(r);
    let top = squash(&page(r, "index.html"));
    assert!(
        top.contains(r#"<li><a href="docs/index.html">A &amp; &lt;b&gt;</a> — x &lt; y</li>"#),
        "{top}"
    );
}

// ---------------------------------------------------------------- FR-094

fn assert_config_error(config: &str) {
    let f = fixture_with_config(config);
    common::write(&f.root, "docs/index.md", index_md());
    common::write(&f.root, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    let o = site(&f.root, &[]);
    assert_eq!(o.code, 1, "config:\n{config}\nstderr: {}", o.stderr);
    assert!(!f.root.join("_site").exists());
}

// [FR-094]
#[test]
fn site設定のタイトルが空白だけなら設定エラーになる() {
    assert_config_error(&cfg_with("site:\n  title: \"   \"\n"));
    assert_config_error(&cfg_with("site:\n  title: \"\"\n"));
}

// [FR-094]
#[test]
fn site設定の要約が空白だけなら設定エラーになる() {
    assert_config_error(&cfg_with("site:\n  description: \" \\t \"\n"));
    assert_config_error(&cfg_with("site:\n  description: \"\"\n"));
}

// [FR-094]
#[test]
fn site設定の値にlfやcrがあれば設定エラーになる() {
    assert_config_error(&cfg_with("site:\n  title: \"a\\nb\"\n"));
    assert_config_error(&cfg_with("site:\n  title: \"a\\rb\"\n"));
    assert_config_error(&cfg_with("site:\n  description: \"a\\nb\"\n"));
    assert_config_error(&cfg_with("site:\n  description: \"a\\rb\"\n"));
    assert_config_error(&cfg_with("site:\n  description: |\n    a\n    b\n"));
}

// [FR-094]
#[test]
fn site設定の値が文字列でなければ設定エラーになる() {
    assert_config_error(&cfg_with("site:\n  title: 123\n"));
    assert_config_error(&cfg_with("site:\n  title: true\n"));
    assert_config_error(&cfg_with("site:\n  title: [a, b]\n"));
    assert_config_error(&cfg_with("site:\n  description: {a: b}\n"));
}

// [FR-094]
#[test]
fn site設定のnullや空の節は未指定として扱われる() {
    for extra in [
        "site:\n",
        "site: null\n",
        "site:\n  title: null\n  description: null\n",
        "site: {}\n",
    ] {
        let f = fixture_with_config(&cfg_with(extra));
        common::write(&f.root, "docs/index.md", index_md());
        let o = site(&f.root, &[]);
        assert_eq!(o.code, 0, "{extra}: {}", o.stderr);
        assert!(
            heading_id(&page(&f.root, "index.html"), 1, "proj").is_some(),
            "{extra}"
        );
    }
}

// [FR-094]
#[test]
fn site設定の値は前後の空白を除いて使われる() {
    let f = fixture_with_config(&cfg_with(
        "site:\n  title: \"  題名  \"\n  description: \"  要約  \"\n",
    ));
    common::write(&f.root, "docs/index.md", index_md());
    site_ok(&f.root);
    let top = page(&f.root, "index.html");
    assert!(top.contains("<title>題名</title>"), "{top}");
    assert!(heading_id(&top, 1, "題名").is_some(), "{top}");
    assert!(meta_description(&top).as_deref() == Some("要約"), "{top}");
}

// [FR-094]
#[test]
fn site設定の検証はsite以外のすべてのコマンドでも行われる() {
    for bad in [
        "site:\n  title: \"  \"\n",
        "site:\n  title: 1\n",
        "site:\n  description: \"a\\nb\"\n",
    ] {
        let f = fixture_with_config(&cfg_with(bad));
        common::write(&f.root, "docs/index.md", index_md());
        common::write(&f.root, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
        for args in [
            vec!["check"],
            vec!["check", "--format", "json"],
            vec!["index"],
            vec!["reconcile", "--all"],
        ] {
            let o = common::run(&f.root, &args).output().unwrap();
            assert_eq!(o.status.code(), Some(1), "{bad} {args:?}");
            let stderr = String::from_utf8_lossy(&o.stderr);
            assert!(stderr.contains("site"), "{bad} {args:?}: {stderr}");
        }
    }
}

// [FR-094]
#[test]
fn 正しいsite設定はsite以外のコマンドの出力に影響しない() {
    let plain = basic();
    let with_site =
        fixture_with_config(&cfg_with("site:\n  title: タイトル\n  description: 要約\n"));
    for f in [&plain, &with_site] {
        common::write(&f.root, "docs/index.md", index_md());
        common::write(
            &f.root,
            "docs/spec.md",
            &doc("DOC-A", "Spec", "[r](../req/fr1.md)\n"),
        );
        common::write(&f.root, "req/fr1.md", &req("FR-001", "要件1", "x\n"));
    }
    for args in [vec!["check", "--format", "json"], vec!["index"]] {
        let a = common::run(&plain.root, &args).output().unwrap();
        let b = common::run(&with_site.root, &args).output().unwrap();
        assert_eq!(a.status.code(), b.status.code(), "{args:?}");
        assert_eq!(a.stdout, b.stdout, "{args:?}");
    }
}

// ---------------------------------------------------------------- FR-095

// [FR-095]
#[test]
fn すべてのページがhtml5の骨格とcspとreferrerを持つ() {
    let f = basic();
    site_ok(&f.root);
    for rel in [
        "index.html",
        "docs/index.html",
        "docs/spec.html",
        "req/fr1.html",
    ] {
        let html = page(&f.root, rel);
        let lower = html.to_ascii_lowercase();
        assert!(
            lower.trim_start().starts_with("<!doctype html>"),
            "{rel}\n{html}"
        );
        assert!(html.contains(r#"<html lang="ja">"#), "{rel}");
        assert!(html.contains(r#"<meta charset="utf-8">"#), "{rel}");
        assert!(html.contains(r#"<meta name="viewport""#), "{rel}");
        assert!(html.contains("<title>"), "{rel}");
        assert!(html.contains(CSP), "{rel}\n{html}");
        assert!(
            html.contains(r#"<meta name="referrer" content="no-referrer">"#),
            "{rel}"
        );
    }
}

// [FR-095]
#[test]
fn ページは外部リソースもスクリプトも含まない() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "<script>alert(1)</script>\n\ntext <script src=\"x.js\"></script>\n",
        ),
    );
    site_ok(&f.root);
    for rel in [
        "index.html",
        "docs/index.html",
        "docs/spec.html",
        "req/fr1.html",
    ] {
        let html = page(&f.root, rel).to_ascii_lowercase();
        assert!(!html.contains("<script"), "{rel}");
        assert!(!html.contains("<link"), "{rel}");
        assert!(!html.contains("@import"), "{rel}");
        assert!(!html.contains("<iframe"), "{rel}");
        assert!(!html.contains("src=\"http"), "{rel}");
        assert!(html.contains("<style"), "{rel}: inline css expected");
    }
}

// [FR-095]
#[test]
fn 内部リンクはすべて相対パスでスラッシュで始まらない() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[r](../req/fr1.md) [i](index.md) [dot](.)\n",
        ),
    );
    site_ok(&f.root);
    for rel in [
        "index.html",
        "docs/index.html",
        "docs/spec.html",
        "req/fr1.html",
    ] {
        let html = page(&f.root, rel);
        assert!(!html.contains(r#"href="/"#), "{rel}\n{html}");
        assert!(!html.contains(r#"src="/"#), "{rel}\n{html}");
    }
    let spec = page(&f.root, "docs/spec.html");
    assert!(spec.contains(r#"href="../req/fr1.html""#), "{spec}");
    assert!(spec.contains(r#"<a href="index.html">i</a>"#), "{spec}");
    assert!(spec.contains(r#"<a href="index.html">dot</a>"#), "{spec}");
}

// [FR-095]
#[test]
fn タイトルと要約はエスケープされmeta_descriptionにも入る() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        "---\ntype: Specification\nid: DOC-A\ntitle: \"A & <B> \\\"q\\\"\"\ndescription: \"x < y & 'z'\"\n---\nbody\n",
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(
        html.contains("<title>A &amp; &lt;B&gt; &quot;q&quot;</title>")
            || html.contains("<title>A &amp; &lt;B&gt; \"q\"</title>"),
        "{html}"
    );
    assert!(
        heading_id(&html, 1, "A &amp; &lt;B&gt; &quot;q&quot;").is_some()
            || heading_id(&html, 1, "A &amp; &lt;B&gt; \"q\"").is_some(),
        "{html}"
    );
    assert!(
        html.contains(r#"<p class="description">x &lt; y &amp; "#),
        "{html}"
    );
    assert!(
        meta_description(&html)
            .unwrap()
            .starts_with("x &lt; y &amp; "),
        "{html}"
    );
    assert!(!html.contains("<B>"), "{html}");
}

// [FR-095]
#[test]
fn 要約が無いとき要約の段落もmeta_descriptionも出ない() {
    let f = basic();
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(meta_description(&html).is_none(), "{html}");
    assert!(!html.contains(r#"class="description""#), "{html}");
}

// [FR-095]
#[test]
fn タイトル要約の順で出てdocとreqのページにはidが表示される() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &common::typed("Spec", Some("要約です"))
            .replace("type: Specification\n", "type: Specification\nid: DOC-A\n"),
    );
    common::write(
        &f.root,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\ntitle: 要件1\ndescription: 要件の要約\n---\n本文\n",
    );
    site_ok(&f.root);
    let spec = page(&f.root, "docs/spec.html");
    let h1 = pos(&spec, "<h1");
    let id = pos(&spec, r#"<p class="id">DOC-A</p>"#);
    let desc = pos(&spec, r#"<p class="description">要約です</p>"#);
    assert!(h1 < id && id < desc, "{spec}");
    let r = page(&f.root, "req/fr1.html");
    let h1 = pos(&r, "<h1");
    let id = pos(&r, r#"<p class="id">FR-001</p>"#);
    let desc = pos(&r, r#"<p class="description">要件の要約</p>"#);
    assert!(h1 < id && id < desc, "{r}");
}

// [FR-095]
#[test]
fn 索引対象文書と索引ページにはidを表示しない() {
    let f = basic();
    common::write(
        &f.root,
        "docs/ref.md",
        "---\ntype: Reference\nid: REF-1\ntitle: 参考\n---\n本文\n",
    );
    site_ok(&f.root);
    assert!(!page(&f.root, "docs/ref.html").contains(r#"class="id""#));
    assert!(!page(&f.root, "docs/index.html").contains(r#"class="id""#));
    assert!(!page(&f.root, "index.html").contains(r#"class="id""#));
}

// ---------------------------------------------------------------- FR-096

// [FR-096]
#[test]
fn タイトルはフロントマターのtitleを優先し無ければ最初のh1で無ければファイル名になる() {
    let f = basic();
    let r = &f.root;
    common::write(
        r,
        "docs/t1.md",
        "---\ntype: Reference\ntitle: FMタイトル\n---\n# 見出し\n",
    );
    common::write(
        r,
        "docs/t2.md",
        "---\ntype: Reference\n---\n本文\n\n# 見出し2\n",
    );
    common::write(r, "docs/t3.md", "---\ntype: Reference\n---\n本文のみ\n");
    common::write(
        r,
        "docs/t4.md",
        "---\ntype: Reference\ntitle: \"  \"\n---\n本文のみ\n",
    );
    site_ok(r);
    assert!(page(r, "docs/t1.html").contains("<title>FMタイトル</title>"));
    assert!(page(r, "docs/t2.html").contains("<title>見出し2</title>"));
    assert!(page(r, "docs/t3.html").contains("<title>t3</title>"));
    assert!(page(r, "docs/t4.html").contains("<title>t4</title>"));
}

// [FR-096]
#[test]
fn reqのタイトルもidではなく文書のタイトル規則で決まる() {
    let f = basic();
    common::write(
        &f.root,
        "req/fr2.md",
        "---\ntype: Requirement\nid: FR-002\n---\n本文\n",
    );
    common::write(
        &f.root,
        "req/fr3.md",
        "---\ntype: Requirement\nid: FR-003\n---\n# 見出し3\n",
    );
    site_ok(&f.root);
    assert!(page(&f.root, "req/fr2.html").contains("<title>fr2</title>"));
    assert!(page(&f.root, "req/fr3.html").contains("<title>見出し3</title>"));
}

// [FR-096]
#[test]
fn 要約は無い空改行を含むときなしで折り畳みスカラーは有効になる() {
    let f = basic();
    let r = &f.root;
    common::write(
        r,
        "docs/d1.md",
        "---\ntype: Reference\ndescription: \"\"\n---\n本文\n",
    );
    common::write(
        r,
        "docs/d2.md",
        "---\ntype: Reference\ndescription: |\n  a\n  b\n---\n本文\n",
    );
    common::write(
        r,
        "docs/d3.md",
        "---\ntype: Reference\ndescription: >\n  a\n  b\n---\n本文\n",
    );
    common::write(
        r,
        "docs/d4.md",
        "---\ntype: Reference\ndescription: \"  前後  \"\n---\n本文\n",
    );
    site_ok(r);
    for rel in ["docs/d1.html", "docs/d2.html"] {
        assert!(meta_description(&page(r, rel)).is_none(), "{rel}");
    }
    assert!((meta_description(&page(r, "docs/d3.html")).as_deref() == Some("a b")));
    assert!((meta_description(&page(r, "docs/d4.html")).as_deref() == Some("前後")));
}

// [FR-096]
#[test]
fn 索引ページのタイトルと要約はディレクトリの規則に従う() {
    let f = fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "---\ntitle: 文書\ndescription: 一覧です\n---\n- [S](s.md)\n",
    );
    common::write(r, "docs/s.md", &reference("S", "x\n"));
    common::write(r, "docs/sub/index.md", "- [T](t.md)\n");
    common::write(r, "docs/sub/t.md", &reference("T", "x\n"));
    site_ok(r);
    let root_index = page(r, "docs/index.html");
    assert!(root_index.contains("<title>文書</title>"), "{root_index}");
    assert!(
        (meta_description(&root_index).as_deref() == Some("一覧です")),
        "{root_index}"
    );
    assert!(
        root_index.contains(r#"<p class="description">一覧です</p>"#),
        "{root_index}"
    );
    let sub = page(r, "docs/sub/index.html");
    assert!(sub.contains("<title>sub</title>"), "{sub}");
    assert!(meta_description(&sub).is_none(), "{sub}");
}

// [FR-096]
#[test]
fn 索引ページは本文の見出しがあってもタイトルはディレクトリ名になる() {
    let f = fixture();
    let r = &f.root;
    common::write(r, "docs/index.md", "# 見出しだけ\n\n- [S](s.md)\n");
    common::write(r, "docs/s.md", &reference("S", "x\n"));
    site_ok(r);
    let html = page(r, "docs/index.html");
    assert!(html.contains("<title>docs</title>"), "{html}");
}

// [FR-096]
#[test]
fn 索引ページはフロントマターを解析できなくても公開されタイトルはディレクトリ名になる() {
    let f = fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "---\ntitle: [\ndescription: 要約\n---\n本文のインデックス\n",
    );
    common::write(r, "docs/s.md", &reference("S", "x\n"));
    let o = site_ok(r);
    let html = page(r, "docs/index.html");
    assert!(html.contains("<title>docs</title>"), "{html}");
    assert!(meta_description(&html).is_none(), "{html}");
    assert!(html.contains("本文のインデックス"), "{html}");
    assert!(!html.contains("description: 要約"), "{html}");
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// ---------------------------------------------------------------- FR-097

// [FR-097]
#[test]
fn 表と取り消し線とタスクリストと脚注が変換される() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n~~消す~~\n\n- [x] 済\n- [ ] 未\n\n脚注あり[^n]\n\n[^n]: 脚注本文\n",
        ),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains("<table>"), "{html}");
    assert!(html.contains("<del>消す</del>"), "{html}");
    assert!(html.contains(r#"type="checkbox""#), "{html}");
    assert!(html.contains("脚注本文"), "{html}");
    assert!(html.contains(r##"href="#n""##), "{html}");
    assert!(!html.contains("type: Specification"), "{html}");
}

// [FR-097]
#[test]
fn インラインの生htmlはエスケープして文字として表示する() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "文 <b>強調</b> と <img src=x onerror=alert(1)> です\n",
        ),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains("&lt;b&gt;強調&lt;/b&gt;"), "{html}");
    assert!(
        html.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "{html}"
    );
    assert!(!html.contains("<b>強調"), "{html}");
    assert!(!html.contains("<img src=x"), "{html}");
}

// [FR-097]
#[test]
fn htmlブロックはエスケープしてpreで囲んで表示する() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "<div class=\"x\">\nHTMLBLOCK\n</div>\n\n後続\n",
        ),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    let pre = pos(&html, "<pre>");
    let end = pos(&html[pre..], "</pre>") + pre;
    let inner = &html[pre..end];
    assert!(
        inner.contains("&lt;div class=&quot;x&quot;&gt;")
            || inner.contains("&lt;div class=\"x\"&gt;"),
        "{inner}"
    );
    assert!(inner.contains("HTMLBLOCK"), "{inner}");
    assert!(inner.contains("&lt;/div&gt;"), "{inner}");
    assert!(!html.contains("<div class=\"x\">"), "{html}");
}

// [FR-097]
#[test]
fn スクリプトのhtmlブロックは実行できない形で表示される() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "<script>\nalert(1)\n</script>\n\n後続\n"),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(!html.contains("<script"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
}

// [FR-097]
#[test]
fn htmlコメントのブロックは複数行でも出力せず警告も出ない() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "前\n\n<!-- 単一行SECRET1 -->\n\n<!--\n複数行SECRET2\n続き\n-->\n\n<!-- 終端後 --> 同行SECRET3\n\n後\n",
        ),
    );
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    for s in ["SECRET1", "SECRET2", "SECRET3", "&lt;!--", "<!--"] {
        assert!(!html.contains(s), "{s}\n{html}");
    }
    assert!(html.contains("前") && html.contains("後"), "{html}");
    assert!(o.stderr.is_empty(), "{}", o.stderr);
}

// [FR-097]
#[test]
fn インラインのhtmlコメントは出力しない() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "前 <!-- INLINESECRET --> 後\n"),
    );
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(!html.contains("INLINESECRET"), "{html}");
    assert!(!html.contains("&lt;!--"), "{html}");
    assert!(o.stderr.is_empty(), "{}", o.stderr);
}

// [FR-097]
#[test]
fn 最初の行がコメントで始まらないブロックの中のコメントはエスケープして表示される() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "<div>\n<!-- KEEPME -->\n</div>\n\n後\n"),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains("&lt;!-- KEEPME --&gt;"), "{html}");
    assert!(!html.contains("<!-- KEEPME"), "{html}");
}

// ---------------------------------------------------------------- FR-098

fn link_fixture(body: &str) -> Fixture {
    let f = basic();
    common::write(&f.root, "docs/spec.md", &doc("DOC-A", "Spec", body));
    f
}

// [FR-098]
#[test]
fn 外部リンクとメールとページ内アンカーはそのまま出力される() {
    let f = link_fixture(
        "[a](https://example.com/p?q=1) [b](http://example.com/) [c](mailto:x@example.com) [d](#sec) [e](HTTPS://Example.com/U) [f](Mailto:y@example.com)\n",
    );
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(
        html.contains(r#"href="https://example.com/p?q=1""#),
        "{html}"
    );
    assert!(html.contains(r#"href="http://example.com/""#), "{html}");
    assert!(html.contains(r#"href="mailto:x@example.com""#), "{html}");
    assert!(html.contains(r##"href="#sec""##), "{html}");
    assert!(html.contains(r#"href="HTTPS://Example.com/U""#), "{html}");
    assert!(html.contains(r#"href="Mailto:y@example.com""#), "{html}");
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-098]
#[test]
fn メールの自動リンクはmailtoのリンクになり素のurlは自動リンクにならない() {
    let f = link_fixture("<a@b.example> と https://bare.example.com/x です\n");
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains(r#"href="mailto:a@b.example""#), "{html}");
    assert!(html.contains("https://bare.example.com/x"), "{html}");
    assert!(
        !html.contains(r#"href="https://bare.example.com/x""#),
        "{html}"
    );
}

// [FR-098]
#[test]
fn 相対mdリンクは出力上の相対パスのhtmlリンクに書き換わる() {
    let f = basic();
    let r = &f.root;
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[s](sub/p.md) [r](../req/fr1.md#top) [u](sub/p.md?x=1)\n",
        ),
    );
    common::write(
        r,
        "docs/sub/p.md",
        &reference("P", "[b](../spec.md) [c](../../req/fr1.md)\n"),
    );
    site_ok(r);
    let spec = page(r, "docs/spec.html");
    assert!(spec.contains(r#"href="sub/p.html""#), "{spec}");
    assert!(spec.contains(r##"href="../req/fr1.html#top""##), "{spec}");
    assert_eq!(spec.matches(r#"href="sub/p.html""#).count(), 2, "{spec}");
    let p = page(r, "docs/sub/p.html");
    assert!(p.contains(r#"href="../spec.html""#), "{p}");
    assert!(p.contains(r#"href="../../req/fr1.html""#), "{p}");
}

// [FR-098]
#[test]
fn ディレクトリを指すリンクは索引ページのindex_htmlになる() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/sub/index.md", "- [P](p.md)\n");
    common::write(r, "docs/sub/p.md", &reference("P", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[a](sub) [b](sub/) [c](.) [d](./)\n"),
    );
    let o = site_ok(r);
    let spec = page(r, "docs/spec.html");
    assert_eq!(
        spec.matches(r#"href="sub/index.html""#).count(),
        2,
        "{spec}"
    );
    for t in ["a", "b"] {
        assert!(
            spec.contains(&format!(r#"<a href="sub/index.html">{t}</a>"#)),
            "{spec}"
        );
    }
    for t in ["c", "d"] {
        assert!(
            spec.contains(&format!(r#"<a href="index.html">{t}</a>"#)),
            "{spec}"
        );
    }
    // header link + `.` + `./`
    assert_eq!(spec.matches(r#"href="index.html""#).count(), 3, "{spec}");
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-098]
#[test]
fn 索引ページの本文にも同じ規則が適用されエントリがそのまま一覧になる() {
    let f = basic();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "- [Spec](spec.md) — 説明\n- [Sub](sub/) — 子\n",
    );
    common::write(r, "docs/sub/index.md", "- [P](p.md)\n");
    common::write(r, "docs/sub/p.md", &reference("P", "x\n"));
    site_ok(r);
    let html = squash(&page(r, "docs/index.html"));
    assert!(
        html.contains(r#"<li><a href="spec.html">Spec</a> — 説明</li>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<li><a href="sub/index.html">Sub</a> — 子</li>"#),
        "{html}"
    );
}

// [FR-099]
#[test]
fn ディレクトリを指すリンクで索引ページが公開されないときはリンクにならず警告になる() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/nodir/x.md", &reference("X", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[NA](nodir) [NB](nodir/)\n"),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    assert!(!html.contains("nodir/index.html"), "{html}");
    assert!(html.contains("NA") && html.contains("NB"), "{html}");
    assert!(
        !html.contains(">NA</a>") && !html.contains(">NB</a>"),
        "{html}"
    );
    // only the two header anchors remain
    assert_eq!(anchor_count(&html), 2, "{html}");
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished link: nodir",
            "warning: docs/spec.md: unpublished link: nodir/",
        ]
    );
}

// ---------------------------------------------------------------- FR-099

// [FR-099]
#[test]
fn 公開されないリンクはリンク文だけを残して警告の書式で出る() {
    let f = link_fixture(
        "[JS](javascript:void) [ABS](/abs/x.md) [OUT](../../../x.md) [NOPE](missing.md) [PLAIN](note.txt) [Q](?q=1)\n",
    );
    common::write(&f.root, "docs/note.txt", "x\n");
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    for text in ["JS", "ABS", "OUT", "NOPE", "PLAIN", "Q"] {
        assert!(html.contains(text), "{text}");
    }
    for bad in [
        "javascript:",
        "/abs/x.md",
        "missing.html",
        "note.txt\"",
        "?q=1\"",
    ] {
        assert!(!html.contains(&format!("href=\"{bad}")), "{bad}\n{html}");
    }
    assert!(!html.contains(r#"href="javascript"#));
    for text in ["JS", "ABS", "OUT", "NOPE", "PLAIN", "Q"] {
        assert!(!html.contains(&format!(">{text}</a>")), "{text}\n{html}");
    }
    // only the two header anchors remain
    assert_eq!(anchor_count(&html), 2, "{html}");
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished link: javascript:void",
            "warning: docs/spec.md: unpublished link: /abs/x.md",
            "warning: docs/spec.md: unpublished link: ../../../x.md",
            "warning: docs/spec.md: unpublished link: missing.md",
            "warning: docs/spec.md: unpublished link: note.txt",
            "warning: docs/spec.md: unpublished link: ?q=1",
        ]
    );
}

// [FR-099]
#[test]
fn 空のリンク先は公開されないリンクとして警告になる() {
    let f = link_fixture("[EMPTY]() 文\n");
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains("EMPTY"), "{html}");
    assert!(
        !html.contains("<a>EMPTY") && !html.contains("href=\"\""),
        "{html}"
    );
    assert!(!html.contains(">EMPTY</a>"), "{html}");
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/spec.md: unpublished link: "]
    );
}

// [FR-099]
#[test]
fn リンク先は山括弧とtitle属性を除いたdestinationで警告に出る() {
    let f = link_fixture("[A](<a b.md>) [B](missing.md \"タイトル\") [C](gone%20x.md)\n");
    let o = site_ok(&f.root);
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished link: a b.md",
            "warning: docs/spec.md: unpublished link: missing.md",
            "warning: docs/spec.md: unpublished link: gone%20x.md",
        ]
    );
}

// [FR-099]
#[test]
fn typeが無い文書とlogmdと除外対象へのリンクは公開されない() {
    let f = fixture_with_config(&cfg_with("exclude:\n  - drafts\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    common::write(r, "docs/untyped.md", "# U\n");
    common::write(r, "docs/log.md", &reference("L", "x\n"));
    common::write(r, "docs/drafts/d.md", &reference("D", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[1](untyped.md) [2](log.md) [3](drafts/d.md)\n",
        ),
    );
    let o = site_ok(r);
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished link: untyped.md",
            "warning: docs/spec.md: unpublished link: log.md",
            "warning: docs/spec.md: unpublished link: drafts/d.md",
        ]
    );
}

// [FR-099]
#[test]
fn 警告はページの元パスのバイト順でページ内は画像とリンクを区別せず出現順になる() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/a0.md", &reference("A0", "[l](n-a0.md)\n"));
    common::write(
        r,
        "docs/a-b.md",
        &reference("AB", "[l1](n1.md) ![i](x.svg) [l2](n2.md)\n"),
    );
    common::write(
        r,
        "docs/a/b.md",
        &reference("A/B", "![i](a.svg) [l](n-slash.md)\n"),
    );
    common::write(r, "docs/Z.md", &reference("Z", "[l](n-z.md)\n"));
    common::write(r, "req/fr1.md", &req("FR-001", "R", "[l](nr.md)\n"));
    let o = site_ok(r);
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/Z.md: unpublished link: n-z.md",
            "warning: docs/a-b.md: unpublished link: n1.md",
            "warning: docs/a-b.md: unpublished image: x.svg",
            "warning: docs/a-b.md: unpublished link: n2.md",
            "warning: docs/a/b.md: unpublished image: a.svg",
            "warning: docs/a/b.md: unpublished link: n-slash.md",
            "warning: docs/a0.md: unpublished link: n-a0.md",
            "warning: req/fr1.md: unpublished link: nr.md",
        ]
    );
}

// ---------------------------------------------------------------- FR-100

fn png_bytes() -> Vec<u8> {
    vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 9, 8, 7]
}

fn write_bin(root: &Path, rel: &str, bytes: &[u8]) {
    let full = root.join(rel);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, bytes).unwrap();
}

// [FR-100]
#[test]
fn 許可された拡張子の画像は大文字小文字を問わず同じ相対位置へコピーされる() {
    let f = basic();
    let r = &f.root;
    let mut body = String::new();
    for (i, name) in ["a.png", "b.PNG", "c.jpg", "d.JPEG", "e.gif", "f.webP"]
        .iter()
        .enumerate()
    {
        write_bin(r, &format!("docs/img/{name}"), &[i as u8, 1, 2, 3]);
        body.push_str(&format!("![alt{i}](img/{name})\n\n"));
    }
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", &body));
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    for (i, name) in ["a.png", "b.PNG", "c.jpg", "d.JPEG", "e.gif", "f.webP"]
        .iter()
        .enumerate()
    {
        assert!(
            html.contains(&format!(r#"src="img/{name}""#)),
            "{name}\n{html}"
        );
        assert!(html.contains(&format!(r#"alt="alt{i}""#)), "{name}\n{html}");
        assert_eq!(
            std::fs::read(r.join(format!("_site/docs/img/{name}"))).unwrap(),
            vec![i as u8, 1, 2, 3]
        );
    }
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-100]
#[test]
fn requirementsの根の配下の画像もコピーされimgから相対参照される() {
    let f = basic();
    let r = &f.root;
    write_bin(r, "req/pic/z.png", &png_bytes());
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "![r](../req/pic/z.png)\n"),
    );
    let o = site_ok(r);
    assert_eq!(
        std::fs::read(r.join("_site/req/pic/z.png")).unwrap(),
        png_bytes()
    );
    assert!(page(r, "docs/spec.html").contains(r#"src="../req/pic/z.png""#));
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-100]
#[test]
fn httpsの画像はコピーせずそのまま参照し警告も出ない() {
    let f = link_fixture("![remote](https://img.example.com/a.png)\n");
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(
        html.contains(r#"src="https://img.example.com/a.png""#),
        "{html}"
    );
    assert!(o.warnings().is_empty(), "{}", o.stderr);
    assert!(!exists(&f.root, "https:"));
}

// [FR-100]
#[test]
fn httpの画像とスラッシュ始まりの画像は公開されない画像として警告になる() {
    let f = link_fixture(
        "![HTTPALT](http://img.example.com/a.png) ![ABSALT](/a.png) ![FTPALT](ftp://x/a.png)\n",
    );
    let o = site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(!html.contains("<img"), "{html}");
    for t in ["HTTPALT", "ABSALT", "FTPALT"] {
        assert!(html.contains(t), "{t}");
    }
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished image: http://img.example.com/a.png",
            "warning: docs/spec.md: unpublished image: /a.png",
            "warning: docs/spec.md: unpublished image: ftp://x/a.png",
        ]
    );
}

// [FR-100]
#[test]
fn svgとその他の拡張子の画像は出さず代替テキストだけを残して警告する() {
    let f = basic();
    let r = &f.root;
    write_bin(r, "docs/a.svg", b"<svg onload=alert(1)/>");
    write_bin(r, "docs/b.bmp", b"BM");
    write_bin(r, "docs/noext", b"x");
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "![SVGALT](a.svg) ![BMPALT](b.bmp) ![NOEXT](noext) ![MISSING](gone.png)\n",
        ),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    assert!(!html.contains("<img"), "{html}");
    for t in ["SVGALT", "BMPALT", "NOEXT", "MISSING"] {
        assert!(html.contains(t), "{t}");
    }
    assert!(!exists(r, "docs/a.svg"));
    assert!(!exists(r, "docs/b.bmp"));
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished image: a.svg",
            "warning: docs/spec.md: unpublished image: b.bmp",
            "warning: docs/spec.md: unpublished image: noext",
            "warning: docs/spec.md: unpublished image: gone.png",
        ]
    );
}

// [FR-100]
#[test]
fn 除外ディレクトリの画像とdocsとrequirementsの外の画像はコピーされない() {
    let f = fixture_with_config(&cfg_with("exclude:\n  - drafts\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    write_bin(r, "docs/drafts/x.png", &png_bytes());
    write_bin(r, "docs/node_modules/y.png", &png_bytes());
    write_bin(r, "assets/z.png", &png_bytes());
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "![A](drafts/x.png) ![B](node_modules/y.png) ![C](../assets/z.png)\n",
        ),
    );
    let o = site_ok(r);
    assert!(!page(r, "docs/spec.html").contains("<img"));
    assert!(!exists(r, "docs/drafts/x.png"));
    assert!(!exists(r, "docs/node_modules/y.png"));
    assert!(!exists(r, "assets/z.png"));
    assert_eq!(o.warnings().len(), 3, "{}", o.stderr);
}

// [FR-100]
#[cfg(unix)]
#[test]
fn シンボリックリンクの画像はコピーされない() {
    let f = basic();
    let r = &f.root;
    write_bin(r, "docs/real.png", &png_bytes());
    std::os::unix::fs::symlink(r.join("docs/real.png"), r.join("docs/link.png")).unwrap();
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", "![L](link.png)\n"));
    let o = site_ok(r);
    assert!(!page(r, "docs/spec.html").contains("<img"));
    assert!(!exists(r, "docs/link.png"));
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/spec.md: unpublished image: link.png"]
    );
}

// [FR-100]
#[test]
fn 画像の相対パスは元ファイルのディレクトリ基準でパーセントデコードされる() {
    let f = basic();
    let r = &f.root;
    write_bin(r, "docs/sub/my pic.png", &png_bytes());
    common::write(r, "docs/sub/index.md", "- [P](p.md)\n");
    common::write(r, "docs/sub/p.md", &reference("P", "![x](my%20pic.png)\n"));
    let o = site_ok(r);
    assert_eq!(
        std::fs::read(r.join("_site/docs/sub/my pic.png")).unwrap(),
        png_bytes()
    );
    assert!(page(r, "docs/sub/p.html").contains(r#"src="my%20pic.png""#));
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// ---------------------------------------------------------------- FR-101

fn req_fixture() -> Fixture {
    let f = basic();
    common::write(&f.root, "req/fr1.md", &req("FR-001", "要件1", "本文\n"));
    f
}

// [FR-101]
#[test]
fn reqページの末尾に参照しているdocの一覧が出る() {
    let f = req_fixture();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[r](../req/fr1.md) もう一度 [r2](../req/fr1.md#x)\n",
        ),
    );
    site_ok(&f.root);
    let html = page(&f.root, "req/fr1.html");
    let sq = squash(&html);
    assert!(
        sq.contains(r#"<h2>この要求を参照している文書</h2><ul><li><a href="../docs/spec.html">Spec</a></li></ul>"#),
        "{html}"
    );
    assert_eq!(html.matches("../docs/spec.html").count(), 1, "{html}");
    assert!(pos(&html, "本文") < pos(&html, "この要求を参照している文書"));
}

// [FR-101]
#[test]
fn 参照元の一覧はdocのリポジトリ相対パスの順でタイトルの順ではない() {
    let f = req_fixture();
    common::write(
        &f.root,
        "docs/b.md",
        &doc("DOC-B", "AA", "[r](../req/fr1.md)\n"),
    );
    common::write(
        &f.root,
        "docs/a.md",
        &doc("DOC-C", "ZZ", "[r](../req/fr1.md)\n"),
    );
    site_ok(&f.root);
    let sq = squash(&page(&f.root, "req/fr1.html"));
    assert!(
        sq.contains(r#"<ul><li><a href="../docs/a.html">ZZ</a></li><li><a href="../docs/b.html">AA</a></li></ul>"#),
        "{sq}"
    );
}

// [FR-101]
#[test]
fn 参照元のタイトルはエスケープされる() {
    let f = req_fixture();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "\"A & <b>\"", "[r](../req/fr1.md)\n"),
    );
    site_ok(&f.root);
    let sq = squash(&page(&f.root, "req/fr1.html"));
    assert!(
        sq.contains(r#"<a href="../docs/spec.html">A &amp; &lt;b&gt;</a>"#),
        "{sq}"
    );
}

// [FR-101]
#[test]
fn 参照しているdocが無いときは見出しごと節を出さない() {
    let f = req_fixture();
    site_ok(&f.root);
    let html = page(&f.root, "req/fr1.html");
    assert!(!html.contains("この要求を参照している文書"), "{html}");
}

// [FR-101]
#[test]
fn docページと索引対象文書と索引ページには参照元の節を出さない() {
    let f = req_fixture();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[r](../req/fr1.md)\n"),
    );
    common::write(
        &f.root,
        "docs/ref.md",
        &reference("参考", "[r](../req/fr1.md)\n"),
    );
    site_ok(&f.root);
    for rel in [
        "docs/spec.html",
        "docs/ref.html",
        "docs/index.html",
        "index.html",
    ] {
        assert!(
            !page(&f.root, rel).contains("この要求を参照している文書"),
            "{rel}"
        );
    }
}

// ---------------------------------------------------------------- FR-102

// [FR-102]
#[test]
fn 非utf8のバイトは置換文字になり失敗しない() {
    let f = basic();
    let mut bytes = b"---\ntype: Specification\nid: DOC-A\ntitle: Spec\n---\nbefore ".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe]);
    bytes.extend_from_slice(b" after\n");
    write_bin(&f.root, "docs/spec.md", &bytes);
    let o = site(&f.root, &[]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains("before \u{fffd}\u{fffd} after"), "{html}");
}

// [FR-102]
#[test]
fn 索引ページの非utf8のバイトも置換文字になる() {
    let f = basic();
    let mut bytes = b"intro ".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b" end\n");
    write_bin(&f.root, "docs/index.md", &bytes);
    let o = site(&f.root, &[]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(page(&f.root, "docs/index.html").contains("intro \u{fffd} end"));
}

// [FR-102]
#[test]
fn フロントマターは本文から取り除かれcrlfの囲みでも同じ() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        "---\r\ntype: Specification\r\nid: DOC-A\r\ntitle: Spec\r\n---\r\n本文CRLF\r\n",
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(html.contains("本文CRLF"), "{html}");
    assert!(!html.contains("type: Specification"), "{html}");
    assert!(!html.contains("id: DOC-A"), "{html}");
}

// [FR-102]
#[test]
fn 先頭行がハイフン3つでなければフロントマターなしとして全体を本文にする() {
    let f = basic();
    common::write(&f.root, "docs/index.md", "\n---\ntitle: 文書\n---\n本文X\n");
    common::write(&f.root, "docs/ref.md", &reference("参考", "x\n"));
    site_ok(&f.root);
    let html = page(&f.root, "docs/index.html");
    assert!(html.contains("<title>docs</title>"), "{html}");
    assert!(html.contains("本文X"), "{html}");
    assert!(html.contains("title: 文書"), "{html}");
}

// [FR-102]
#[test]
fn 閉じのハイフンが無い文書は公開されず索引ページは全体を本文として公開される() {
    let f = basic();
    common::write(
        &f.root,
        "docs/unterm.md",
        "---\ntype: Specification\nid: DOC-U\ntitle: U\n本文U\n",
    );
    common::write(&f.root, "docs/index.md", "---\ntitle: 文書\nINDEXBODY\n");
    let o = site(&f.root, &[]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(!exists(&f.root, "docs/unterm.html"));
    let html = page(&f.root, "docs/index.html");
    assert!(html.contains("<title>docs</title>"), "{html}");
    assert!(meta_description(&html).is_none(), "{html}");
    assert!(html.contains("INDEXBODY"), "{html}");
    assert!(html.contains("title: 文書"), "{html}");
}

// [FR-102]
#[test]
fn 閉じのハイフンが無い型付き文書へのリンクは公開されないリンクになる() {
    let f = basic();
    common::write(
        &f.root,
        "docs/unterm.md",
        "---\ntype: Reference\ntitle: U\n本文U\n",
    );
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[u](unterm.md)\n"),
    );
    let o = site_ok(&f.root);
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/spec.md: unpublished link: unterm.md"]
    );
}

// ---------------------------------------------------------------- FR-103

// [FR-103]
#[test]
fn 根にindexmdが無いときエントリはリンクなしのディレクトリ名になり警告が出る() {
    let f = fixture_with_config(
        &cfg_with("").replace("index:\n  - docs\n", "index:\n  - docs/z\n  - docs/a\n"),
    );
    let r = &f.root;
    common::write(r, "docs/z/x.md", &reference("X", "x\n"));
    common::write(r, "docs/a/y.md", &reference("Y", "[b](nope.md)\n"));
    let o = site(r, &[]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/z/index.md: missing index",
            "warning: docs/a/index.md: missing index",
            "warning: docs/a/y.md: unpublished link: nope.md",
        ]
    );
    let top = squash(&page(r, "index.html"));
    assert!(top.contains("<li>z</li>"), "{top}");
    assert!(top.contains("<li>a</li>"), "{top}");
    assert!(!top.contains("docs/z/index.html"), "{top}");
    assert!(!exists(r, "docs/z/index.html"));
    assert!(exists(r, "docs/z/x.html"));
}

// [FR-103]
#[test]
fn 根にindexmdが無くても根の文書へのリンクは通常どおり書き換わる() {
    let f = fixture();
    let r = &f.root;
    common::write(r, "docs/x.md", &reference("X", "[s](y.md)\n"));
    common::write(r, "docs/y.md", &reference("Y", "x\n"));
    let o = site_ok(r);
    assert_eq!(o.warnings(), vec!["warning: docs/index.md: missing index"]);
    assert!(page(r, "docs/x.html").contains(r#"href="y.html""#));
}

// ---------------------------------------------------------------- FR-104

// [FR-104]
#[test]
fn docページのヘッダにトップと担当索引へのリンクが順に出る() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    common::write(
        r,
        "docs/sub/index.md",
        "---\ntitle: サブ\n---\n- [P](p.md)\n",
    );
    common::write(r, "docs/sub/p.md", &reference("P", "x\n"));
    site_ok(r);
    let spec = page(r, "docs/spec.html");
    let top = pos(&spec, r#"<a href="../index.html">サイト名</a>"#);
    let idx = pos(&spec, r#"<a href="index.html">文書</a>"#);
    assert!(top < idx, "{spec}");
    let p = page(r, "docs/sub/p.html");
    let top = pos(&p, r#"<a href="../../index.html">サイト名</a>"#);
    let idx = pos(&p, r#"<a href="index.html">サブ</a>"#);
    assert!(top < idx, "{p}");
    assert!(!p.contains(r#"<a href="../index.html">文書</a>"#), "{p}");
}

// [FR-104]
#[test]
fn 索引ページのヘッダは親の索引へのリンクを持つ() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    common::write(
        r,
        "docs/sub/index.md",
        "---\ntitle: サブ\n---\n- [P](p.md)\n",
    );
    common::write(r, "docs/sub/p.md", &reference("P", "x\n"));
    site_ok(r);
    let sub = page(r, "docs/sub/index.html");
    let top = pos(&sub, r#"<a href="../../index.html">サイト名</a>"#);
    let parent = pos(&sub, r#"<a href="../index.html">文書</a>"#);
    assert!(top < parent, "{sub}");
}

// [FR-104]
#[test]
fn 根の索引ページとreqと根の外のdocにはトップへのリンクだけを置く() {
    let f = fixture_with_config(
        &cfg_with("site:\n  title: サイト名\n")
            .replace("index:\n  - docs\n", "index:\n  - docs/guide\n"),
    );
    let r = &f.root;
    common::write(
        r,
        "docs/guide/index.md",
        "---\ntitle: ガイド\n---\n- [G](g.md)\n",
    );
    common::write(r, "docs/guide/g.md", &reference("G", "x\n"));
    common::write(r, "docs/other.md", &doc("DOC-O", "Other", "x\n"));
    common::write(r, "req/fr1.md", &req("FR-001", "要件1", "x\n"));
    site_ok(r);

    let root_index = page(r, "docs/guide/index.html");
    assert!(
        root_index.contains(r#"<a href="../../index.html">サイト名</a>"#),
        "{root_index}"
    );
    assert!(!root_index.contains(">ガイド</a>"), "{root_index}");

    let other = page(r, "docs/other.html");
    assert!(
        other.contains(r#"<a href="../index.html">サイト名</a>"#),
        "{other}"
    );
    assert!(!other.contains("guide/index.html"), "{other}");

    let rq = page(r, "req/fr1.html");
    assert!(
        rq.contains(r#"<a href="../index.html">サイト名</a>"#),
        "{rq}"
    );
    assert!(!rq.contains("docs/guide/index.html"), "{rq}");
}

// [FR-104]
#[test]
fn 根に公開された索引ページが無いときはトップへのリンクだけになる() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    let r = &f.root;
    common::write(r, "docs/x.md", &doc("DOC-X", "X", "x\n"));
    site_ok(r);
    let html = page(r, "docs/x.html");
    assert!(
        html.contains(r#"<a href="../index.html">サイト名</a>"#),
        "{html}"
    );
    assert!(!html.contains(r#"href="index.html""#), "{html}");
}

// [FR-104]
#[test]
fn トップページ自身にはヘッダのリンクを置かない() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    common::write(&f.root, "docs/index.md", index_md());
    common::write(&f.root, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    site_ok(&f.root);
    let top = page(&f.root, "index.html");
    assert!(!top.contains(r#"<a href="index.html""#), "{top}");
    assert!(!top.contains(">サイト名</a>"), "{top}");
}

// [FR-104]
#[test]
fn ヘッダのリンクの文はエスケープされる() {
    let f = fixture_with_config(&cfg_with("site:\n  title: \"A & B\"\n"));
    common::write(
        &f.root,
        "docs/index.md",
        "---\ntitle: \"C < D\"\n---\n- [S](spec.md)\n",
    );
    common::write(&f.root, "docs/spec.md", &doc("DOC-A", "Spec", "x\n"));
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(
        html.contains(r#"<a href="../index.html">A &amp; B</a>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<a href="index.html">C &lt; D</a>"#),
        "{html}"
    );
}

// ---------------------------------------------------------------- FR-105

// [FR-105]
#[test]
fn 最初のブロックがatxのh1のときテンプレートのh1を出さず本文の見出しを使う() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        "---\ntype: Specification\nid: DOC-A\ntitle: FMタイトル\ndescription: 要約文\n---\n# 本文の見出し\n\n続き\n",
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert_eq!(html.matches("<h1").count(), 1, "{html}");
    assert!(html.contains("<title>FMタイトル</title>"), "{html}");
    assert!(
        (heading_id(&html, 1, "本文の見出し") == Some(Some("本文の見出し".to_string()))),
        "{html}"
    );
    assert!(heading_id(&html, 1, "FMタイトル").is_none(), "{html}");
    let h1 = pos(&html, "</h1>");
    let id = pos(&html, r#"<p class="id">DOC-A</p>"#);
    let desc = pos(&html, r#"<p class="description">要約文</p>"#);
    let rest = pos(&html, "続き");
    assert!(h1 < id && id < desc && desc < rest, "{html}");
}

// [FR-105]
#[test]
fn 最初のブロックがsetextのh1のときも本文の見出しを使う() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "FM", "Setext見出し\n============\n\n続き\n"),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert_eq!(html.matches("<h1").count(), 1, "{html}");
    assert!(
        (heading_id(&html, 1, "Setext見出し") == Some(Some("setext見出し".to_string()))),
        "{html}"
    );
}

// [FR-105]
#[test]
fn 出力しないコメントのブロックは最初のブロックに数えない() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "FM", "<!-- コメント -->\n\n# 見出し\n\n続き\n"),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert_eq!(html.matches("<h1").count(), 1, "{html}");
    assert!(
        (heading_id(&html, 1, "見出し") == Some(Some("見出し".to_string()))),
        "{html}"
    );
}

// [FR-105]
#[test]
fn 最初のブロックがh1でなければテンプレートのh1を出す() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "テンプレ & タイトル", "前置き\n\n# 後の見出し\n"),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(
        heading_id(&html, 1, "テンプレ &amp; タイトル").is_some(),
        "{html}"
    );
    assert!(html.contains("後の見出し"), "{html}");
    assert!(pos(&html, "<h1") < pos(&html, "前置き"));
}

// [FR-105]
#[test]
fn 最初のブロックがh2のときもテンプレートのh1を出す() {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "題名", "## 小見出し\n\n本文\n"),
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/spec.html");
    assert!(heading_id(&html, 1, "題名").is_some(), "{html}");
    assert!(
        (heading_id(&html, 2, "小見出し") == Some(Some("小見出し".to_string()))),
        "{html}"
    );
}

// [FR-105]
#[test]
fn 本文の最初が見出しのときも要約はその見出しの直後に置かれる() {
    let f = basic();
    common::write(
        &f.root,
        "docs/index.md",
        "---\ntitle: 文書\ndescription: 一覧です\n---\n# 一覧の見出し\n\n- [Spec](spec.md)\n",
    );
    site_ok(&f.root);
    let html = page(&f.root, "docs/index.html");
    assert!(html.contains("<title>文書</title>"), "{html}");
    assert_eq!(html.matches("<h1").count(), 1, "{html}");
    assert!(
        pos(&html, "</h1>") < pos(&html, r#"<p class="description">一覧です</p>"#),
        "{html}"
    );
    assert!(
        pos(&html, r#"<p class="description">一覧です</p>"#) < pos(&html, "Spec</a>"),
        "{html}"
    );
}

// ---------------------------------------------------------------- FR-106

fn heading_page(body: &str) -> String {
    let f = basic();
    common::write(
        &f.root,
        "docs/spec.md",
        &doc("DOC-A", "Spec", &format!("前置き\n\n{body}")),
    );
    site_ok(&f.root);
    page(&f.root, "docs/spec.html")
}

// [FR-106]
#[test]
fn 見出しにはgithub互換のid属性が付く() {
    let html = heading_page("## Hello, World!\n\n### 日本語 の_見出し-1\n");
    assert!(
        (heading_id(&html, 2, "Hello, World!") == Some(Some("hello-world".to_string()))),
        "{html}"
    );
    assert!(
        heading_ids(&html, 3) == vec![Some("日本語-の_見出し-1".to_string())],
        "{html}"
    );
}

// [FR-106]
#[test]
fn コードスパンの中身を連結したプレーンテキストからidが決まる() {
    let html = heading_page("## `Foo` bar *Baz* [link](https://e.example)\n");
    assert!(
        heading_ids(&html, 2) == vec![Some("foo-bar-baz-link".to_string())],
        "{html}"
    );
}

// [FR-106]
#[test]
fn 同じ値の見出しは2つ目から連番のサフィックスが付く() {
    let html = heading_page("## Dup\n\n## Dup\n\n## Dup\n");
    let ids: Vec<Option<String>> = ["dup", "dup-1", "dup-2"]
        .iter()
        .map(|s| Some(s.to_string()))
        .collect();
    assert_eq!(heading_ids(&html, 2), ids, "{html}");
}

// [FR-106]
#[test]
fn 規則を当てた結果が空の見出しにはidを付けない() {
    let html = heading_page("## !!!\n\n## ???\n");
    assert_eq!(heading_id(&html, 2, "!!!"), Some(None), "{html}");
    assert_eq!(heading_id(&html, 2, "???"), Some(None), "{html}");
}

// [FR-106]
#[test]
fn 脚注のラベルは使用済みとして扱い見出しのidと重ならないようにする() {
    let html = heading_page("## A\n\n本文[^a]\n\n[^a]: 脚注\n");
    assert!(
        (heading_id(&html, 2, "A") == Some(Some("a-1".to_string()))),
        "{html}"
    );
    assert!(
        heading_ids(&html, 2) == vec![Some("a-1".to_string())],
        "{html}"
    );
}

// [FR-106]
#[test]
fn 見出しidへのページ内リンクはそのまま出力される() {
    let html = heading_page("## 概要\n\n[戻る](#概要)\n");
    assert!(
        heading_ids(&html, 2) == vec![Some("概要".to_string())],
        "{html}"
    );
    assert!(html.contains(r##"href="#%E6%A6%82%E8%A6%81""##), "{html}");
}

// ---------------------------------------------------------------- FR-107

// [FR-107]
#[test]
fn リンクのクエリは捨てられパーセントデコードしてから解決される() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/a b.md", &reference("AB", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[q](a%20b.md?plain=1) [f](a%20b.md#sec) [qf](a%20b.md?x=1#sec)\n",
        ),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    assert_eq!(html.matches(r#"href="a%20b.html""#).count(), 1, "{html}");
    assert_eq!(
        html.matches(r##"href="a%20b.html#sec""##).count(),
        2,
        "{html}"
    );
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-107]
#[test]
fn hrefは非ascii文字をセグメントごとにパーセントエンコードする() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/日本語.md", &reference("日本語", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[j](日本語.md) [e](%E6%97%A5%E6%9C%AC%E8%AA%9E.md)\n",
        ),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    assert_eq!(
        html.matches(r#"href="%E6%97%A5%E6%9C%AC%E8%AA%9E.html""#)
            .count(),
        2,
        "{html}"
    );
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-107]
#[test]
fn デコード後に現れるシャープはフラグメントとして再分割しない() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/a#b.md", &reference("AB", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[h](a%23b.md#f)\n"),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    assert!(html.contains(r##"href="a%23b.html#f""##), "{html}");
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-107]
#[test]
fn hrefの最初のセグメントのコロンは3aになる() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/a:b.md", &reference("AB", "x\n"));
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", "[c](./a:b.md)\n"));
    let o = site_ok(r);
    assert!(page(r, "docs/spec.html").contains(r#"href="a%3Ab.html""#));
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-107]
#[test]
fn クエリを捨てた結果が空のリンクは空のリンクとして公開されない() {
    let f = link_fixture("[x](?only=1) [y](#) 文\n");
    let o = site_ok(&f.root);
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/spec.md: unpublished link: ?only=1"]
    );
}

// [FR-107]
#[test]
fn 親ディレクトリを辿るリンクは正規化されて解決される() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/a/b/deep.md", &reference("Deep", "[up](../../spec.md) [self](./deep.md) [out](../../../req/fr1.md) [esc](../../../../x.md)\n"));
    common::write(r, "docs/a/index.md", "- [B](b/)\n");
    common::write(r, "docs/a/b/index.md", "- [D](deep.md)\n");
    let o = site_ok(r);
    let html = page(r, "docs/a/b/deep.html");
    assert!(html.contains(r#"href="../../spec.html""#), "{html}");
    assert!(html.contains(r#"href="deep.html""#), "{html}");
    assert!(html.contains(r#"href="../../../req/fr1.html""#), "{html}");
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/a/b/deep.md: unpublished link: ../../../../x.md"]
    );
}

// [FR-095]
#[test]
fn ヘッダと一覧と参照元一覧のhrefも同じエンコード規則に従う() {
    let f = fixture_with_config(
        &cfg_with("").replace("index:\n  - docs\n", "index:\n  - docs/日本 語\n"),
    );
    let r = &f.root;
    common::write(
        r,
        "docs/日本 語/index.md",
        "---\ntitle: 日本\n---\n- [P](p%20q.md)\n",
    );
    common::write(
        r,
        "docs/日本 語/p q.md",
        &doc("DOC-P", "PQ", "[r](../../req/fr1.md)\n"),
    );
    common::write(r, "req/fr1.md", &req("FR-001", "要件1", "x\n"));
    site_ok(r);
    let top = squash(&page(r, "index.html"));
    assert!(
        top.contains(r#"<a href="docs/%E6%97%A5%E6%9C%AC%20%E8%AA%9E/index.html">日本</a>"#),
        "{top}"
    );
    let p = page(r, "docs/日本 語/p q.html");
    assert!(p.contains(r#"href="../../index.html""#), "{p}");
    assert!(p.contains(r#"href="index.html""#), "{p}");
    assert!(p.contains(r#"href="../../req/fr1.html""#), "{p}");
    let rq = squash(&page(r, "req/fr1.html"));
    assert!(
        rq.contains(
            r#"<li><a href="../docs/%E6%97%A5%E6%9C%AC%20%E8%AA%9E/p%20q.html">PQ</a></li>"#
        ),
        "{rq}"
    );
    for html in [&top, &p, &rq] {
        for h in all_hrefs(html) {
            assert!(h.is_ascii() && !h.contains(' '), "href not encoded: {h}");
        }
    }
}

// [FR-107]
#[test]
fn パーセント記号を含むファイル名とアンパサンドはhrefで符号化される() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/100%.md", &reference("P", "x\n"));
    common::write(r, "docs/a&b.md", &reference("AB", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[p](100%25.md) [a](a&b.md)\n"),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html");
    assert!(html.contains(r#"href="100%25.html""#), "{html}");
    assert!(html.contains(r#"href="a&amp;b.html""#), "{html}");
    assert!(o.warnings().is_empty(), "{}", o.stderr);
}

// [FR-107]
#[test]
fn フラグメントはrfc3986で許される文字以外をパーセントエンコードする() {
    let f = basic();
    let r = &f.root;
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "[a](spec.md#概要) [b](#概要) [c](spec.md#a/b?c) [x](#%E6%A6%82) [y](spec.md#100%)\n",
        ),
    );
    site_ok(r);
    let html = page(r, "docs/spec.html");
    assert!(
        html.contains(r##"href="spec.html#%E6%A6%82%E8%A6%81""##),
        "{html}"
    );
    assert!(html.contains(r##"href="#%E6%A6%82%E8%A6%81""##), "{html}");
    assert!(html.contains(r##"href="spec.html#a/b?c""##), "{html}");
    assert!(html.contains(r##"href="#%E6%A6%82""##), "{html}");
    assert!(html.contains(r##"href="spec.html#100%25""##), "{html}");
}

// ------------------------------------------------------- 追加分（レビュー反映）

fn all_hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("href=\"") {
        rest = &rest[i + 6..];
        let end = rest.find('"').unwrap();
        out.push(rest[..end].to_string());
    }
    out
}

// [FR-099]
#[test]
fn スクリプトや属性注入の経路はリンクにならず警告になる() {
    let f = basic();
    let r = &f.root;
    write_bin(r, "docs/pic.png", &png_bytes());
    common::write(
        r,
        "docs/spec.md",
        &doc(
            "DOC-A",
            "Spec",
            "<javascript:alert(1)>\n\n[u](JAVASCRIPT:alert)\n\n[f](<#\" onmouseover=x>)\n\n[t](spec.md \"a\\\" onmouseover=\\\"b\")\n\n![a\" onerror=\"x](pic.png)\n\n[e](//evil.example/x)\n\n[c](a:b.md)\n",
        ),
    );
    let o = site_ok(r);
    let html = page(r, "docs/spec.html").to_lowercase();
    assert!(!html.contains("href=\"javascript"), "{html}");
    assert!(!html.contains("\" onmouseover="), "{html}");
    assert!(!html.contains("\" onerror="), "{html}");
    assert!(!html.contains("href=\"//evil"), "{html}");
    assert!(!html.contains("href=\"a:b"), "{html}");
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished link: javascript:alert(1)",
            "warning: docs/spec.md: unpublished link: JAVASCRIPT:alert",
            "warning: docs/spec.md: unpublished link: //evil.example/x",
            "warning: docs/spec.md: unpublished link: a:b.md",
        ]
    );
}

// [FR-091]
#[test]
fn idが不正なspecificationも索引の根の配下なら公開されidは表示されない() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/badid.md", &doc("bad id!", "BadId", "本文\n"));
    site_ok(r);
    let html = page(r, "docs/badid.html");
    assert!(!html.contains(r#"class="id""#), "{html}");
}

// [FR-091]
#[test]
fn id重複のrequirementは索引の根の外ならページにならない() {
    let f = basic();
    let r = &f.root;
    common::write(r, "req/d1.md", &req("FR-009", "D1", "x\n"));
    common::write(r, "req/d2.md", &req("FR-009", "D2", "x\n"));
    site_ok(r);
    assert!(!exists(r, "req/d1.html"));
    assert!(!exists(r, "req/d2.html"));
    assert!(exists(r, "req/fr1.html"));
}

// [FR-099]
#[test]
fn ページにならないreqへのリンクは公開されないリンクになる() {
    let f = basic();
    let r = &f.root;
    common::write(r, "req/d1.md", &req("FR-009", "D1", "x\n"));
    common::write(r, "req/d2.md", &req("FR-009", "D2", "x\n"));
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "[a](../req/d1.md) [b](../req/d2.md)\n"),
    );
    let o = site_ok(r);
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/spec.md: unpublished link: ../req/d1.md",
            "warning: docs/spec.md: unpublished link: ../req/d2.md",
        ]
    );
}

// [FR-101]
#[test]
fn ノードでないdocやidが重複するreqからの参照は参照元一覧に出ない() {
    let f = req_fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/badid.md",
        &doc("bad id!", "BadId", "[r](../req/fr1.md)\n"),
    );
    common::write(r, "req/d1.md", &req("FR-009", "D1", "[r](fr1.md)\n"));
    common::write(r, "req/d2.md", &req("FR-009", "D2", "x\n"));
    site_ok(r);
    assert!(!page(r, "req/fr1.html").contains("この要求を参照している文書"));
}

// [FR-101]
#[test]
fn referenceやreqからの参照は参照元一覧に出ない() {
    let f = req_fixture();
    let r = &f.root;
    common::write(r, "docs/ref.md", &reference("参考", "[r](../req/fr1.md)\n"));
    common::write(r, "req/fr2.md", &req("FR-002", "要件2", "[r](fr1.md)\n"));
    site_ok(r);
    let html = page(r, "req/fr1.html");
    assert!(!html.contains("この要求を参照している文書"), "{html}");
    assert!(!html.contains("../docs/ref.html"), "{html}");
}

// [FR-104]
#[test]
fn 階層を飛ばした文書は最も近い公開された索引へリンクする() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    common::write(r, "docs/a/b/c.md", &reference("C", "x\n"));
    site_ok(r);
    let c = page(r, "docs/a/b/c.html");
    assert!(
        c.contains(r#"<a href="../../../index.html">サイト名</a>"#),
        "{c}"
    );
    assert!(c.contains(r#"<a href="../../index.html">文書</a>"#), "{c}");
}

// [FR-104]
#[test]
fn 索引ページの親は階層を飛ばした最も近い祖先の索引になる() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    let r = &f.root;
    common::write(r, "docs/index.md", index_md());
    common::write(
        r,
        "docs/a/b/index.md",
        "---\ntitle: ビー\n---\n- [C](c.md)\n",
    );
    common::write(r, "docs/a/b/c.md", &reference("C", "x\n"));
    site_ok(r);
    let b = page(r, "docs/a/b/index.html");
    assert!(
        b.contains(r#"<a href="../../../index.html">サイト名</a>"#),
        "{b}"
    );
    assert!(b.contains(r#"<a href="../../index.html">文書</a>"#), "{b}");
    assert!(!b.contains(">ビー</a>"), "{b}");
}

// [FR-104]
#[test]
fn 根にindexmdが無く子の索引だけあるとき公開されない索引にはリンクしない() {
    let f = fixture_with_config(&cfg_with("site:\n  title: サイト名\n"));
    let r = &f.root;
    common::write(
        r,
        "docs/sub/index.md",
        "---\ntitle: サブ\n---\n- [P](p.md)\n",
    );
    common::write(r, "docs/sub/p.md", &reference("P", "x\n"));
    common::write(r, "docs/x.md", &reference("X", "x\n"));
    let o = site_ok(r);
    assert_eq!(o.warnings(), vec!["warning: docs/index.md: missing index"]);
    let p = page(r, "docs/sub/p.html");
    assert!(
        p.contains(r#"<a href="../../index.html">サイト名</a>"#),
        "{p}"
    );
    assert!(p.contains(r#"<a href="index.html">サブ</a>"#), "{p}");
    let sub = page(r, "docs/sub/index.html");
    assert!(
        sub.contains(r#"<a href="../../index.html">サイト名</a>"#),
        "{sub}"
    );
    assert!(!sub.contains(r#"href="../index.html""#), "{sub}");
    let x = page(r, "docs/x.html");
    assert!(x.contains(r#"<a href="../index.html">サイト名</a>"#), "{x}");
    assert!(!x.contains(r#"href="index.html""#), "{x}");
}

// [FR-091]
#[test]
fn 根の外のindexmdと根が別なら根の外のindexmdは公開されずリンクは警告になる() {
    let f = fixture_with_config(
        &cfg_with("").replace("index:\n  - docs\n", "index:\n  - docs/guide\n"),
    );
    let r = &f.root;
    common::write(r, "docs/guide/index.md", "- [G](g.md)\n");
    common::write(r, "docs/guide/g.md", &reference("G", "x\n"));
    common::write(r, "docs/index.md", &reference("DocsIndex", "x\n"));
    common::write(
        r,
        "req/index.md",
        "---\ntype: Requirement\nid: FR-005\n---\nx\n",
    );
    common::write(
        r,
        "docs/other.md",
        &doc("DOC-O", "Other", "[a](../req/) [b](index.md)\n"),
    );
    let o = site_ok(r);
    assert!(!exists(r, "docs/index.html"));
    assert!(!exists(r, "req/index.html"));
    assert_eq!(
        o.warnings(),
        vec![
            "warning: docs/other.md: unpublished link: ../req/",
            "warning: docs/other.md: unpublished link: index.md",
        ]
    );
}

// [FR-100]
#[test]
fn testsの根の配下の画像はコピーされず警告になる() {
    let f = basic();
    let r = &f.root;
    write_bin(r, "tests/shot.png", &png_bytes());
    common::write(
        r,
        "docs/spec.md",
        &doc("DOC-A", "Spec", "![T](../tests/shot.png)\n"),
    );
    let o = site_ok(r);
    assert!(!exists(r, "tests/shot.png"));
    assert!(!page(r, "docs/spec.html").contains("<img"));
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/spec.md: unpublished image: ../tests/shot.png"]
    );
}

// [FR-090]
#[test]
fn indexを空配列で明示してもエラーで何も書かない() {
    let f = fixture_with_config(&cfg_with("").replace("index:\n  - docs\n", "index: []\n"));
    let o = site(&f.root, &[]);
    assert_eq!(o.code, 1);
    assert!(
        o.stderr
            .contains("index is not configured in .miwa/config.yaml"),
        "{}",
        o.stderr
    );
    assert!(!f.root.join("_site").exists());
}

// [FR-089]
#[test]
fn out_に絶対パスを指定できる() {
    let f = basic();
    let abs = f._tmp.path().join("abs_out");
    let o = site(&f.root, &["--out", abs.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(abs.join("index.html").is_file());
    assert!(!f.root.join("_site").exists());
}

// [FR-091]
#[cfg(unix)]
#[test]
fn シンボリックリンクのmdは公開されずリンクは警告になる() {
    let f = basic();
    let r = &f.root;
    common::write(r, "docs/real.md", &reference("Real", "x\n"));
    std::os::unix::fs::symlink(r.join("docs/real.md"), r.join("docs/link.md")).unwrap();
    common::write(r, "docs/spec.md", &doc("DOC-A", "Spec", "[l](link.md)\n"));
    let o = site_ok(r);
    assert!(exists(r, "docs/real.html"));
    assert!(!exists(r, "docs/link.html"));
    assert_eq!(
        o.warnings(),
        vec!["warning: docs/spec.md: unpublished link: link.md"]
    );
}

// [FR-093]
#[test]
fn 根のindexmdのフロントマターが解析できないときトップのエントリはディレクトリ名で要約なしになる() {
    let f = fixture();
    let r = &f.root;
    common::write(
        r,
        "docs/index.md",
        "---\ntitle: [\ndescription: 要約\n---\n",
    );
    common::write(r, "docs/x.md", &reference("X", "x\n"));
    site_ok(r);
    let top = squash(&page(r, "index.html"));
    assert!(
        top.contains(r#"<li><a href="docs/index.html">docs</a></li>"#),
        "{top}"
    );
}
