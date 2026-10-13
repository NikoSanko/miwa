mod common;

use std::path::{Path, PathBuf};

const CFG: &str = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\nindex:\n  - docs\n";

const CSP: &str = r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self' data: https:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">"#;

const NOID_HEAD: &str = "documents in the diff have no id; add \"id:\" to their front matter:";

struct Repo {
    tmp: tempfile::TempDir,
    root: PathBuf,
}

/// Repository root named `proj` holding the default config, an index page,
/// one doc (`docs/spec.md`, DOC-A) and one req (`req/fr1.md`, FR-001).
/// Nothing is committed yet; call `base()` to create `main` and branch `feature`.
fn repo() -> Repo {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    common::write(&root, ".miwa/config.yaml", CFG);
    common::write(&root, "docs/index.md", index_md());
    common::write(
        &root,
        "docs/spec.md",
        &doc("DOC-A", "仕様A", "本文です。\n"),
    );
    common::write(&root, "req/fr1.md", &req("FR-001", "要件1", "要件本文。\n"));
    Repo { tmp, root }
}

impl Repo {
    /// Commits everything on `main` and switches to a new `feature` branch.
    fn base(&self) {
        common::git_init(&self.root);
        common::git_commit(&self.root, "base");
        common::git_checkout_new_branch(&self.root, "feature");
    }

    fn write(&self, rel: &str, content: &str) {
        common::write(&self.root, rel, content);
    }

    fn write_bin(&self, rel: &str, bytes: &[u8]) {
        let full = self.root.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, bytes).unwrap();
    }

    fn commit(&self) {
        common::git_commit(&self.root, "feature change");
    }

    fn rm(&self, rel: &str) {
        std::fs::remove_file(self.root.join(rel)).unwrap();
    }

    /// An output path inside the temp dir but outside the repository.
    fn out(&self, name: &str) -> String {
        self.tmp.path().join(name).to_string_lossy().into_owned()
    }

    fn diff(&self, args: &[&str]) -> Out {
        let mut full = vec!["site", "--diff"];
        full.extend_from_slice(args);
        let o = common::run(&self.root, &full).output().unwrap();
        Out {
            code: o.status.code().unwrap(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// Runs `site --diff --out <tmp>/o.html <args>`, expects exit 0 and the output file.
    fn html_with(&self, args: &[&str]) -> String {
        let out = self.out("o.html");
        let mut full = vec!["--out", out.as_str()];
        full.extend_from_slice(args);
        let o = self.diff(&full);
        assert_eq!(o.code, 0, "stderr: {}", o.stderr);
        assert_eq!(o.stdout, "", "stdout must be empty");
        std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("missing output: {e}"))
    }

    fn html(&self) -> String {
        self.html_with(&[])
    }

    /// Like `html_with` but writes to a second output path (so it can follow `html_with`).
    fn html_with_second(&self, args: &[&str]) -> String {
        let out = self.out("o2.html");
        let _ = std::fs::remove_file(&out);
        let mut full = vec!["--out", out.as_str()];
        full.extend_from_slice(args);
        let o = self.diff(&full);
        assert_eq!(o.code, 0, "stderr: {}", o.stderr);
        std::fs::read_to_string(&out).unwrap()
    }

    /// Runs expecting exit 0 with no output file written.
    fn expect_no_file(&self, args: &[&str]) {
        let out = self.out("none.html");
        let mut full = vec!["--out", out.as_str()];
        full.extend_from_slice(args);
        let o = self.diff(&full);
        assert_eq!(o.code, 0, "stderr: {}", o.stderr);
        assert_eq!(o.stdout, "");
        assert!(!Path::new(&out).exists());
    }

    /// Runs expecting exit 1, empty stdout and no output file; returns stderr.
    fn expect_error(&self, args: &[&str]) -> String {
        let out = self.out("err.html");
        let mut full = vec!["--out", out.as_str()];
        full.extend_from_slice(args);
        let o = self.diff(&full);
        assert_eq!(o.code, 1, "stderr: {}", o.stderr);
        assert_eq!(o.stdout, "");
        assert!(!Path::new(&out).exists(), "no file must be written");
        assert!(!o.stderr.trim().is_empty());
        o.stderr
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn index_md() -> &'static str {
    "---\ntitle: 文書\ndescription: 文書の一覧\n---\n- [仕様A](spec.md) — 説明\n"
}

fn doc(id: &str, title: &str, body: &str) -> String {
    format!("---\ntype: BusinessNeed\nid: {id}\ntitle: {title}\n---\n{body}")
}

fn req(id: &str, title: &str, body: &str) -> String {
    format!("---\ntype: Requirement\nid: {id}\ntitle: {title}\n---\n{body}")
}

fn reference(title: &str, body: &str) -> String {
    format!("---\ntype: Reference\ntitle: {title}\n---\n{body}")
}

fn reference_id(id: &str, title: &str, body: &str) -> String {
    format!("---\ntype: Reference\nid: {id}\ntitle: {title}\n---\n{body}")
}

fn spec(body: &str) -> String {
    doc("DOC-A", "仕様A", body)
}

fn png(tag: u8) -> Vec<u8> {
    vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, tag, 8, 7]
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

fn data_uri(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", b64(bytes))
}

/// Removes the highlight `<span ...>` / `</span>` tags.
fn strip_spans(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find("<span class=\"d") {
        out.push_str(&rest[..i]);
        let end = rest[i..].find('>').unwrap();
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    out.replace("</span>", "")
}

fn count(html: &str, needle: &str) -> usize {
    html.matches(needle).count()
}

/// The frame of document `n` (from `<details class="file" id="dN"` to the next frame).
fn sec(html: &str, n: usize) -> &str {
    let open = format!(r#"<details class="file" id="d{n}" open>"#);
    let start = html
        .find(&open)
        .unwrap_or_else(|| panic!("frame d{n} not found in:\n{html}"));
    let rest = &html[start + open.len()..];
    let end = rest
        .find(r#"<details class="file" id="d"#)
        .map(|i| start + open.len() + i)
        .unwrap_or(html.len());
    &html[start..end]
}

fn frames(html: &str) -> usize {
    count(html, r#"<details class="file" id="d"#)
}

fn short(root: &Path, rev: &str) -> String {
    common::run_git(root, &["rev-parse", rev])[..7].to_string()
}

/// A body of `n` distinct paragraphs.
fn paras(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("段落{i}の本文です。")).collect()
}

fn join(blocks: &[String]) -> String {
    blocks.join("\n\n") + "\n"
}

/// A repo whose spec body has the given blocks at base and `edit` applied on the feature branch.
fn spec_edit(base_blocks: &[String], feature_blocks: &[String]) -> Repo {
    let r = repo();
    r.write("docs/spec.md", &spec(&join(base_blocks)));
    r.base();
    r.write("docs/spec.md", &spec(&join(feature_blocks)));
    r.commit();
    r
}

fn bar_counts(frame: &str) -> (usize, usize, usize) {
    let start = frame.find(r#"<span class="bar">"#).expect("bar");
    let rest = &frame[start..];
    let end = rest.find("</span>").expect("bar end");
    let bar = &rest[..end];
    (
        count(bar, r#"<i class="a">"#),
        count(bar, r#"<i class="d">"#),
        count(bar, "<i></i>"),
    )
}

fn changed_spec() -> Repo {
    let r = repo();
    r.base();
    r.write("docs/spec.md", &spec("変更後の本文です。\n"));
    r.commit();
    r
}

// ---------------------------------------------------------------- FR-108

// [FR-108]
#[test]
fn 出力先の既定はリポジトリルートの_site_diff_htmlで標準出力は空() {
    let r = changed_spec();
    let o = r.diff(&[]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout, "");
    assert!(r.root.join("_site-diff.html").is_file());
    assert!(!r.root.join("_site").exists());
}

// [FR-108]
#[test]
fn 未コミットの変更が変更後に含まれる() {
    let r = repo();
    r.base();
    r.write("docs/spec.md", &spec("未コミットの新しい本文。\n"));
    let html = r.html();
    assert!(html.contains("未コミットの新しい本文。"), "{html}");
    assert!(html.contains(r#"<span class="st mod">"#));
}

// [FR-108]
#[test]
fn 未追跡のファイルが変更後に含まれる() {
    let r = repo();
    r.base();
    r.write(
        "docs/new.md",
        &doc("DOC-N", "新規文書", "追加された本文。\n"),
    );
    let html = r.html();
    assert!(html.contains("追加された本文。"), "{html}");
    assert!(html.contains(r#"<span class="st add">"#));
}

// [FR-108]
#[test]
fn 存在しないbaseはexit_1で何も書かない() {
    let r = changed_spec();
    r.expect_error(&["--base", "no-such-ref"]);
}

// [FR-108]
#[test]
fn 共通の祖先が無いときはexit_1で何も書かない() {
    let r = repo();
    r.base();
    common::run_git(&r.root, &["checkout", "--orphan", "island"]);
    r.write("docs/spec.md", &spec("孤立した履歴。\n"));
    common::git_commit(&r.root, "orphan");
    r.expect_error(&[]);
}

// [FR-108]
#[test]
fn baseでdiff_baseを上書きできる() {
    let r = repo();
    r.write(
        ".miwa/config.yaml",
        &CFG.replace("diff_base: main", "diff_base: no-such-ref"),
    );
    r.base();
    r.write("docs/spec.md", &spec("変更後。\n"));
    r.commit();
    r.expect_error(&[]);
    let html = r.html_with(&["--base", "main"]);
    assert!(html.contains("変更後。"));
}

// [FR-108]
#[test]
fn diffなしのbase_link_base_head_labelはclapのエラーで終了コード2() {
    let r = changed_spec();
    for args in [
        vec!["site", "--base", "main"],
        vec!["site", "--link-base", "https://e.com/"],
        vec!["site", "--head-label", "x"],
    ] {
        let o = common::run(&r.root, &args).output().unwrap();
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(o.stdout.is_empty(), "{args:?}");
    }
    assert!(!r.root.join("_site").exists());
    assert!(!r.root.join("_site-diff.html").exists());
}

// [FR-108]
#[test]
fn diffを付けないsiteの動作は変わらない() {
    let r = changed_spec();
    common::run(&r.root, &["site"]).assert().success();
    assert!(r.root.join("_site/index.html").is_file());
    assert!(!r.root.join("_site-diff.html").exists());
}

// ---------------------------------------------------------------- FR-109

// [FR-109]
#[test]
fn outの相対パスはリポジトリルートからで親ディレクトリが作られる() {
    let r = changed_spec();
    let o = r.diff(&["--out", "build/deep/d.html"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(r.root.join("build/deep/d.html").is_file());
}

// [FR-109]
#[test]
fn outは絶対パスと親参照を含むパスも使える() {
    let r = changed_spec();
    let abs = r.out("abs.html");
    assert_eq!(r.diff(&["--out", &abs]).code, 0);
    assert!(Path::new(&abs).is_file());
    assert_eq!(r.diff(&["--out", "../up.html"]).code, 0);
    assert!(r.tmp.path().join("up.html").is_file());
}

// [FR-109]
#[test]
fn 出力先に通常のファイルがあるとexit_1で上書きしない() {
    let r = changed_spec();
    r.write("keep.html", "KEEP");
    let o = r.diff(&["--out", "keep.html"]);
    assert_eq!(o.code, 1);
    assert_eq!(o.stdout, "");
    assert!(!o.stderr.trim().is_empty());
    assert_eq!(
        std::fs::read_to_string(r.root.join("keep.html")).unwrap(),
        "KEEP"
    );
}

// [FR-109]
#[test]
fn 出力先にディレクトリがあるとexit_1で中身を残す() {
    let r = changed_spec();
    r.write("outdir/keep.txt", "KEEP");
    let o = r.diff(&["--out", "outdir"]);
    assert_eq!(o.code, 1);
    assert_eq!(o.stdout, "");
    assert_eq!(
        std::fs::read_to_string(r.root.join("outdir/keep.txt")).unwrap(),
        "KEEP"
    );
}

// [FR-109]
#[cfg(unix)]
#[test]
fn 出力先がシンボリックリンクでもexit_1でリンク先を変えない() {
    let r = changed_spec();
    r.write("target.txt", "TARGET");
    std::os::unix::fs::symlink("target.txt", r.root.join("link.html")).unwrap();
    let o = r.diff(&["--out", "link.html"]);
    assert_eq!(o.code, 1);
    assert_eq!(
        std::fs::read_to_string(r.root.join("target.txt")).unwrap(),
        "TARGET"
    );
}

// [FR-109]
#[cfg(unix)]
#[test]
fn 出力先が壊れたシンボリックリンクでもexit_1() {
    let r = changed_spec();
    std::os::unix::fs::symlink("missing-target", r.root.join("broken.html")).unwrap();
    let o = r.diff(&["--out", "broken.html"]);
    assert_eq!(o.code, 1);
    assert_eq!(o.stdout, "");
    assert!(!r.root.join("missing-target").exists());
    assert!(std::fs::symlink_metadata(r.root.join("broken.html")).is_ok());
}

// [FR-109]
#[test]
fn 変更が無くても出力先が存在すればexit_1() {
    let r = repo();
    r.base();
    r.write("keep.html", "KEEP");
    let o = r.diff(&["--out", "keep.html"]);
    assert_eq!(o.code, 1);
    assert_eq!(
        std::fs::read_to_string(r.root.join("keep.html")).unwrap(),
        "KEEP"
    );
}

// [FR-109]
#[test]
fn 出力先の確認は比較より前に行われる() {
    let r = repo();
    r.base();
    // would be an id error if the comparison ran first
    r.write(
        "docs/spec.md",
        "---\ntype: BusinessNeed\ntitle: 仕様A\n---\n変更。\n",
    );
    r.write("keep.html", "KEEP");
    let o = r.diff(&["--out", "keep.html"]);
    assert_eq!(o.code, 1);
    assert!(!o.stderr.contains(NOID_HEAD), "{}", o.stderr);
}

// ---------------------------------------------------------------- FR-110

// [FR-110]
#[test]
fn 対象の変更が無ければファイルを作らずexit_0() {
    let r = repo();
    r.base();
    r.expect_no_file(&[]);
}

// [FR-110]
#[test]
fn 文書でないファイルだけの変更は対象にならない() {
    let r = repo();
    r.base();
    r.write("README.md", "ただのメモ\n");
    r.write("src/x.rs", "fn main() {}\n");
    r.expect_no_file(&[]);
}

// [FR-110]
// [FR-112]
#[test]
fn idの欠落エラーは変更なしの扱いより優先される() {
    let r = repo();
    r.base();
    // only the id is removed: not a target, but still an error
    r.write(
        "docs/spec.md",
        "---\ntype: BusinessNeed\ntitle: 仕様A\n---\n本文です。\n",
    );
    let e = r.expect_error(&[]);
    assert!(e.contains("docs/spec.md"), "{e}");
}

// ---------------------------------------------------------------- FR-111

// [FR-111]
#[test]
fn 変更後の設定にindexが無いとexit_1() {
    let r = repo();
    r.base();
    r.write(".miwa/config.yaml", &CFG.replace("index:\n  - docs\n", ""));
    r.write("docs/spec.md", &spec("変更。\n"));
    r.expect_error(&[]);
}

// [FR-111]
#[test]
fn 変更前に設定が無いときはすべて追加になる() {
    let r = repo();
    std::fs::remove_file(r.root.join(".miwa/config.yaml")).unwrap();
    r.write("README.md", "readme\n");
    r.base();
    r.write(".miwa/config.yaml", CFG);
    r.commit();
    let html = r.html();
    assert!(html.contains(r#"<span class="st add">"#));
    assert!(!html.contains(r#"<span class="st mod">"#));
    assert!(!html.contains(r#"<span class="st del">"#));
    assert!(frames(&html) >= 2, "{html}");
}

// [FR-111]
#[test]
fn 変更前の設定にindexが無いときの公開範囲はdocとreqだけ() {
    let r = repo();
    r.write(".miwa/config.yaml", &CFG.replace("index:\n  - docs\n", ""));
    r.write(
        "docs/ref.md",
        &reference_id("REF-1", "参考資料", "参考の本文。\n"),
    );
    r.base();
    r.write(".miwa/config.yaml", CFG);
    r.commit();
    let html = r.html();
    // ref.md and the index page become published; the doc and the req stay as they were
    assert!(html.contains("参考の本文。"), "{html}");
    assert!(!html.contains("本文です。"), "{html}");
    assert!(!html.contains("要件本文。"), "{html}");
}

// [FR-111]
#[test]
fn 本文だけの変更は対象になる() {
    let r = changed_spec();
    let html = r.html();
    assert_eq!(frames(&html), 1);
    assert!(html.contains(r#"<span class="st mod">"#));
}

// [FR-111]
#[test]
fn titleだけの変更は対象になる() {
    let r = repo();
    r.base();
    r.write("docs/spec.md", &doc("DOC-A", "新しい題名", "本文です。\n"));
    let html = r.html();
    assert_eq!(frames(&html), 1);
    assert!(html.contains("新しい題名"));
    assert!(html.contains("仕様A"));
}

// [FR-111]
#[test]
fn descriptionだけの変更は対象になる() {
    let r = repo();
    r.write(
        "docs/spec.md",
        "---\ntype: BusinessNeed\nid: DOC-A\ntitle: 仕様A\ndescription: 旧要約\n---\n本文です。\n",
    );
    r.base();
    r.write(
        "docs/spec.md",
        "---\ntype: BusinessNeed\nid: DOC-A\ntitle: 仕様A\ndescription: 新要約\n---\n本文です。\n",
    );
    let html = r.html();
    assert_eq!(frames(&html), 1);
    let plain = strip_spans(&html);
    assert!(plain.contains("旧要約") && plain.contains("新要約"));
    assert!(html.contains(r#"<span class="da">"#), "{html}");
}

// [FR-111]
#[test]
fn 追加と削除と移動は対象になり変更の無い文書は含まれない() {
    let r = repo();
    r.write(
        "docs/gone.md",
        &doc("DOC-G", "消える文書", "消える本文。\n"),
    );
    r.write("docs/mv.md", &doc("DOC-M", "動く文書", "動く本文。\n"));
    r.base();
    r.rm("docs/gone.md");
    std::fs::rename(r.root.join("docs/mv.md"), r.root.join("docs/mv2.md")).unwrap();
    r.write("docs/new.md", &doc("DOC-N", "新しい文書", "新しい本文。\n"));
    let html = r.html();
    assert_eq!(frames(&html), 3, "{html}");
    for s in ["st add", "st del", "st move"] {
        assert!(html.contains(&format!(r#"<span class="{s}">"#)), "{s}");
    }
    assert!(!html.contains("要件本文。"));
}

// [FR-111]
// [FR-112]
#[test]
fn idを付けただけの変更は対象にならずエラーにもならない() {
    let r = repo();
    r.write("docs/ref.md", &reference("参考", "参考本文。\n"));
    r.base();
    r.write(
        "docs/ref.md",
        &reference_id("REF-1", "参考", "参考本文。\n"),
    );
    r.expect_no_file(&[]);
}

// [FR-111]
#[test]
fn 参照する画像の中身だけの差し替えは対象になる() {
    let r = repo();
    r.write("docs/spec.md", &spec("![図](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.base();
    r.write_bin("docs/pic/a.png", &png(2));
    let html = r.html();
    assert_eq!(frames(&html), 1);
    assert!(html.contains(&data_uri(&png(1))));
    assert!(html.contains(&data_uri(&png(2))));
}

// ---------------------------------------------------------------- FR-112

// [FR-112]
#[test]
fn 同じidでパスが違えば移動として旧パス_新パスを出す() {
    let r = repo();
    r.base();
    std::fs::rename(r.root.join("docs/spec.md"), r.root.join("docs/moved.md")).unwrap();
    let html = r.html();
    assert!(html.contains(r#"<span class="st move">"#));
    assert!(html.contains("docs/spec.md → docs/moved.md"), "{html}");
    assert!(!html.contains(r#"<span class="st add">"#));
    assert!(!html.contains(r#"<span class="st del">"#));
}

// [FR-112]
#[test]
fn 分類をまたぐtypeの変更は削除と追加になる() {
    let r = repo();
    r.base();
    r.write(
        "docs/spec.md",
        "---\ntype: Reference\nid: DOC-A\ntitle: 仕様A\n---\n本文です。\n",
    );
    let html = r.html();
    assert!(html.contains(r#"<span class="st del">"#));
    assert!(html.contains(r#"<span class="st add">"#));
    assert!(!html.contains(r#"<span class="st mod">"#));
    assert!(!html.contains(r#"<span class="st move">"#));
}

// [FR-112]
#[test]
fn 分類をまたぐ同じidは別の文書で重複エラーにならない() {
    let r = repo();
    r.write(
        "docs/ref.md",
        &reference_id("DOC-A", "参考", "参考本文。\n"),
    );
    r.base();
    r.write("docs/spec.md", &spec("変更後の本文。\n"));
    let html = r.html();
    assert_eq!(frames(&html), 1);
    assert!(!html.contains("参考本文。"));
}

// [FR-112]
#[test]
fn 同じ分類で同じidの重複はexit_1() {
    let r = repo();
    r.base();
    r.write("docs/dup.md", &doc("DOC-A", "重複", "重複本文。\n"));
    r.write("docs/spec.md", &spec("変更。\n"));
    r.expect_error(&[]);
}

// [FR-112]
#[test]
fn 重複は差分の対象でなくてもexit_1() {
    let r = repo();
    r.write("docs/dup.md", &doc("DOC-A", "重複", "重複本文。\n"));
    r.base();
    r.write("req/fr1.md", &req("FR-001", "要件1", "別の本文。\n"));
    r.expect_error(&[]);
}

// [FR-112]
#[test]
fn 一般文書の同じidの重複もexit_1() {
    let r = repo();
    r.write("docs/r1.md", &reference_id("R", "一", "1\n"));
    r.write("docs/r2.md", &reference_id("R", "二", "2\n"));
    r.base();
    r.write("req/fr1.md", &req("FR-001", "要件1", "別の本文。\n"));
    r.expect_error(&[]);
}

// [FR-112]
#[test]
fn 変更後にidが無い対象文書はexit_1でメッセージの形が決まっている() {
    let r = repo();
    r.write("docs/ref.md", &reference("参考", "旧。\n"));
    r.base();
    r.write("docs/ref.md", &reference("参考", "新。\n"));
    let e = r.expect_error(&[]);
    assert_eq!(e.trim_end(), format!("{NOID_HEAD}\ndocs/ref.md"));
}

// [FR-112]
#[test]
fn id無しの対象文書は全件をパス順に列挙してから止まる() {
    let r = repo();
    r.base();
    r.write("docs/b.md", &reference("B", "b\n"));
    r.write("docs/sub/c.md", &reference("C", "c\n"));
    r.write("docs/a.md", &reference("A", "a\n"));
    r.write("req/z.md", "---\ntype: Requirement\n---\nz\n");
    let e = r.expect_error(&[]);
    assert_eq!(
        e.trim_end(),
        format!("{NOID_HEAD}\ndocs/a.md\ndocs/b.md\ndocs/sub/c.md\nreq/z.md")
    );
}

// [FR-112]
#[test]
fn 変更前にあったidを消した文書は内容が同じでもexit_1() {
    let r = repo();
    r.write("docs/ref.md", &reference_id("REF-1", "参考", "本文。\n"));
    r.base();
    r.write("docs/ref.md", &reference("参考", "本文。\n"));
    let e = r.expect_error(&[]);
    assert_eq!(e.trim_end(), format!("{NOID_HEAD}\ndocs/ref.md"));
}

// [FR-112]
#[test]
fn 索引の根の外にあるreqのidを消してもエラーになる() {
    let r = repo();
    r.write(
        ".miwa/config.yaml",
        &CFG.replace("  - req\n", "  - req\n  - req2\n"),
    );
    r.write("req2/out.md", &req("FR-009", "外の要件", "本文。\n"));
    r.base();
    r.write(
        "req2/out.md",
        "---\ntype: Requirement\ntitle: 外の要件\n---\n本文。\n",
    );
    let e = r.expect_error(&[]);
    assert!(e.contains("req2/out.md"), "{e}");
}

// [FR-112]
#[test]
fn idが不正でノードにならないdocも変更前後の候補として扱う() {
    let r = repo();
    r.write(
        "docs/bad.md",
        "---\ntype: BusinessNeed\nid: \"\"\ntitle: 不正\n---\n本文。\n",
    );
    r.base();
    r.write(
        "docs/bad.md",
        "---\ntype: BusinessNeed\nid: \"\"\ntitle: 不正\n---\n変更。\n",
    );
    let e = r.expect_error(&[]);
    assert!(e.contains("docs/bad.md"), "{e}");
}

// [FR-112]
#[test]
fn 変更前の側だけでidが無い文書は同じパスの相手が無ければ削除として出る() {
    let r = repo();
    r.write("docs/old.md", &reference("古い", "古い本文。\n"));
    r.base();
    r.rm("docs/old.md");
    let html = r.html();
    assert!(html.contains(r#"<span class="st del">"#));
    assert!(html.contains("古い本文。"));
}

// [FR-112]
#[test]
fn 変更前だけidが無い文書は同じパスの相手と対になる() {
    let r = repo();
    r.write("docs/ref.md", &reference("参考", "旧い本文です。\n"));
    r.base();
    r.write(
        "docs/ref.md",
        &reference_id("REF-1", "参考", "新しい本文です。\n"),
    );
    let html = r.html();
    assert_eq!(frames(&html), 1);
    assert!(html.contains(r#"<span class="st mod">"#));
    assert!(!html.contains(r#"<span class="st add">"#));
}

// [FR-112]
#[test]
fn 両側でidが無く内容が変わらない文書はエラーにならない() {
    let r = repo();
    r.write("docs/noid.md", &reference("idなし", "そのまま。\n"));
    r.base();
    r.write("docs/spec.md", &spec("変更。\n"));
    let html = r.html();
    assert_eq!(frames(&html), 1);
    assert!(!html.contains("docs/noid.md"));
}

// [FR-112]
#[test]
fn 同じパスで対になりidを足して本文も変えると変更として出る() {
    let r = repo();
    r.write("docs/ref.md", &reference("参考", "古い参考本文。\n"));
    r.base();
    r.write(
        "docs/ref.md",
        &reference_id("REF-1", "参考", "新しい参考本文。\n"),
    );
    let html = r.html();
    assert!(html.contains(r#"<span class="st mod">"#));
}

// ---------------------------------------------------------------- FR-113

// [FR-113]
#[test]
fn 追加された索引ページは差分の対象になる() {
    let r = repo();
    r.base();
    r.write(
        "docs/sub/index.md",
        "---\ntitle: 子\n---\n- [子文書](c.md)\n",
    );
    r.write("docs/sub/c.md", &doc("DOC-C", "子文書", "子の本文。\n"));
    let html = r.html();
    assert!(html.contains("docs/sub/index.md"), "{html}");
}

// [FR-113]
#[test]
fn 削除された索引ページは差分の対象になる() {
    let r = repo();
    r.write("docs/sub/index.md", "---\ntitle: 子\n---\n");
    r.base();
    r.rm("docs/sub/index.md");
    let html = r.html();
    assert!(html.contains("docs/sub/index.md"));
    assert!(html.contains(r#"<span class="st del">"#));
}

// [FR-113]
#[test]
fn 元のmarkdownが変わった索引ページは差分の対象になる() {
    let r = repo();
    r.base();
    r.write(
        "docs/index.md",
        "---\ntitle: 文書\ndescription: 変更後の一覧\n---\n- [仕様A](spec.md) — 説明\n",
    );
    let html = r.html();
    assert!(html.contains("docs/index.md"));
    assert!(html.contains(r#"<span class="st mod">"#));
}

// [FR-113]
#[test]
fn 内容が同じ索引ページは対象にならない() {
    let r = repo();
    r.base();
    r.write("docs/spec.md", &spec("変更。\n"));
    let html = r.html();
    assert!(!html.contains("docs/index.md"));
}

// [FR-113]
#[test]
fn ディレクトリを移動した索引ページは旧パスの削除と新パスの追加になる() {
    let r = repo();
    r.write("docs/sub/index.md", "---\ntitle: 子\n---\n");
    r.base();
    std::fs::create_dir_all(r.root.join("docs/sub2")).unwrap();
    std::fs::rename(
        r.root.join("docs/sub/index.md"),
        r.root.join("docs/sub2/index.md"),
    )
    .unwrap();
    let html = r.html();
    assert!(html.contains(r#"<span class="st del">"#));
    assert!(html.contains(r#"<span class="st add">"#));
    assert!(!html.contains(r#"<span class="st move">"#));
}

// [FR-113]
#[test]
fn 索引ページはid検査の対象にならない() {
    let r = repo();
    r.base();
    r.write(
        "docs/index.md",
        "---\ntitle: 文書\n---\n- [仕様A](spec.md)\n- 追加\n",
    );
    let o = r.diff(&["--out", &r.out("i.html")]);
    assert_eq!(o.code, 0, "{}", o.stderr);
}

// ---------------------------------------------------------------- FR-114

// [FR-114]
#[test]
fn htmlの骨格とcspとreferrerを持ちスクリプトも外部リソースも無い() {
    let r = changed_spec();
    let html = r.html();
    let lower = html.to_ascii_lowercase();
    assert!(lower.trim_start().starts_with("<!doctype html>"));
    assert!(html.contains(r#"<html lang="ja">"#));
    assert!(html.contains(r#"<meta charset="utf-8">"#));
    assert!(html.contains(r#"<meta name="viewport""#));
    assert!(html.contains(r#"<meta name="referrer" content="no-referrer">"#));
    assert!(html.contains(CSP), "{html}");
    assert!(!lower.contains("<script"));
    assert!(!lower.contains("<link"));
    assert!(!lower.contains("@import"));
    assert!(!lower.contains("<iframe"));
    assert!(!lower.contains("src=\"http"));
    assert!(lower.contains("prefers-color-scheme"));
}

// [FR-114]
#[test]
fn titleは_base_から_head_label_ダッシュ_siteのタイトルの形() {
    let r = changed_spec();
    let html = r.html();
    assert!(
        html.contains("<title>main → feature — proj</title>"),
        "{html}"
    );
}

// [FR-114]
#[test]
fn titleのsiteタイトルはsite設定に従いエスケープされる() {
    let r = repo();
    r.write(
        ".miwa/config.yaml",
        &format!("{CFG}site:\n  title: \"○○ & <サイト>\"\n"),
    );
    r.base();
    r.write("docs/spec.md", &spec("変更。\n"));
    let html = r.html_with(&["--head-label", "x"]);
    assert!(
        html.contains("<title>main → x — ○○ &amp; &lt;サイト&gt;</title>"),
        "{html}"
    );
}

// [FR-114]
#[test]
fn 同じ入力からは別の出力先でも同じバイト列になる() {
    let r = repo();
    r.base();
    r.write("docs/spec.md", &spec("変更後の本文。\n\n追加の段落。\n"));
    r.write("docs/new.md", &doc("DOC-N", "新規", "新規本文。\n"));
    let a = r.out("a.html");
    let b = r.out("b.html");
    assert_eq!(r.diff(&["--out", &a]).code, 0);
    assert_eq!(r.diff(&["--out", &b]).code, 0);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
}

// [FR-114]
#[test]
fn 生のhtmlは文字のまま表示されコメントは出力されない() {
    let r = repo();
    r.base();
    r.write(
        "docs/spec.md",
        &spec(
            "<script>alert(1)</script>\n\n<b>太字</b>\n\n<!-- 秘密のコメント -->\n\n本文です。\n",
        ),
    );
    let html = r.html();
    assert!(!html.to_ascii_lowercase().contains("<script"));
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{html}"
    );
    assert!(html.contains("&lt;b&gt;"));
    assert!(!html.contains("秘密のコメント"));
}

// [FR-114]
#[test]
fn 未公開リンクや未公開画像の警告は出ない() {
    let r = repo();
    r.base();
    r.write(
        "docs/spec.md",
        &spec("[消える](missing.md) ![ALT](x.svg) ![ALT2](gone.png) [外](../README.md)\n"),
    );
    let out = r.out("w.html");
    let o = r.diff(&["--out", &out, "--link-base", "https://e.com/"]);
    assert_eq!(o.code, 0);
    assert_eq!(o.stderr, "");
    assert_eq!(o.stdout, "");
}

// [FR-114]
#[test]
fn reqページの参照元文書の一覧は出さない() {
    let r = repo();
    r.write("docs/spec.md", &spec("[要件](../req/fr1.md)\n"));
    r.base();
    r.write(
        "req/fr1.md",
        &req("FR-001", "要件1", "変更された要件本文。\n"),
    );
    let html = r.html();
    assert!(html.contains("要件本文。"));
    assert!(!html.contains("この要求を参照している文書"));
}

// ---------------------------------------------------------------- FR-115

// [FR-115]
#[test]
fn ヘッダにsiteタイトルと比較の2点が七文字のshaで出る() {
    let r = changed_spec();
    let base_sha = short(&r.root, "main");
    let head_sha = short(&r.root, "HEAD");
    let html = r.html();
    let want = format!("main ({base_sha}) → feature ({head_sha})");
    assert!(html.contains(&want), "{want}\n{html}");
    assert!(html.contains("proj"));
}

// [FR-115]
#[test]
fn merge_baseの短縮shaは分岐点でheadと別になる() {
    let r = repo();
    r.base();
    common::run_git(&r.root, &["checkout", "main"]);
    r.write("docs/other.md", &doc("DOC-O", "別", "別の本文。\n"));
    common::git_commit(&r.root, "main moves on");
    common::run_git(&r.root, &["checkout", "feature"]);
    r.write("docs/spec.md", &spec("変更。\n"));
    r.commit();
    let mb = common::run_git(&r.root, &["merge-base", "main", "HEAD"])[..7].to_string();
    let head_sha = short(&r.root, "HEAD");
    let html = r.html();
    assert!(
        html.contains(&format!("main ({mb}) → feature ({head_sha})")),
        "{html}"
    );
    assert!(!html.contains("別の本文。"));
}

// [FR-115]
#[test]
fn head_labelを渡すとヘッダに使われbaseは指定の文字列のまま出る() {
    let r = changed_spec();
    let head_sha = short(&r.root, "HEAD");
    let html = r.html_with(&["--head-label", "pr-42", "--base", "refs/heads/main"]);
    let mb = short(&r.root, "main");
    assert!(
        html.contains(&format!("refs/heads/main ({mb}) → pr-42 ({head_sha})")),
        "{html}"
    );
}

// [FR-115]
#[test]
fn detached_headではhead_label省略時にheadと出る() {
    let r = changed_spec();
    common::run_git(&r.root, &["checkout", "--detach"]);
    let head_sha = short(&r.root, "HEAD");
    let html = r.html();
    assert!(html.contains(&format!("→ HEAD ({head_sha})")), "{html}");
}

// [FR-115]
#[test]
fn head_labelの改行や制御文字はexit_1() {
    let r = changed_spec();
    for bad in ["a\nb", "a\rb", "a\u{7}b", "a\u{1b}[0m"] {
        r.expect_error(&["--head-label", bad]);
    }
}

// [FR-115]
#[test]
fn head_labelはhtmlエスケープされて表示される() {
    let r = changed_spec();
    let html = r.html_with(&["--head-label", "<b>&\"x\""]);
    assert!(html.contains("&lt;b&gt;&amp;"), "{html}");
    assert!(!html.contains("<b>&"));
}

// [FR-115]
#[test]
fn 状態アイコンは変更_追加_削除_移動の4種類で件数がヘッダに出る() {
    let r = repo();
    r.write(
        "docs/gone.md",
        &doc("DOC-G", "消える文書", "消える本文。\n"),
    );
    r.write("docs/mv.md", &doc("DOC-M", "動く文書", "動く本文。\n"));
    r.base();
    r.write("docs/spec.md", &spec("変更後。\n"));
    r.rm("docs/gone.md");
    std::fs::rename(r.root.join("docs/mv.md"), r.root.join("docs/mv2.md")).unwrap();
    r.write("docs/new.md", &doc("DOC-N", "新しい文書", "新しい本文。\n"));
    let html = r.html();
    for (cls, icon) in [("mod", "•"), ("add", "+"), ("del", "−"), ("move", "→")] {
        let tag = format!(r#"<span class="st {cls}">{icon}</span>"#);
        // header counter + tree + frame heading
        assert!(count(&html, &tag) >= 3, "{tag}\n{html}");
    }
}

// [FR-115]
#[test]
fn ツリーはディレクトリを先にパス順で並べアンカーがd1から採番される() {
    let r = repo();
    r.base();
    r.write("docs/zeta.md", &doc("DOC-Z", "ゼータ", "z\n"));
    r.write("docs/alpha.md", &doc("DOC-AL", "アルファ", "a\n"));
    r.write("docs/sub/mid.md", &doc("DOC-MID", "中間", "m\n"));
    r.write("req/fr1.md", &req("FR-001", "要件1", "変更。\n"));
    let html = r.html();
    assert_eq!(frames(&html), 4);
    assert!(sec(&html, 1).contains("docs/sub/mid.md"));
    assert!(sec(&html, 2).contains("docs/alpha.md"));
    assert!(sec(&html, 3).contains("docs/zeta.md"));
    assert!(sec(&html, 4).contains("req/fr1.md"));
    for n in 1..=4 {
        assert!(html.contains(&format!("href=\"#d{n}\"")), "d{n}");
    }
    let p: Vec<usize> = (1..=4)
        .map(|n| html.find(&format!("id=\"d{n}\"")).unwrap())
        .collect();
    assert!(p.windows(2).all(|w| w[0] < w[1]));
}

// ---------------------------------------------------------------- FR-116

// [FR-116]
#[test]
fn 文書の枠はアンカー付きのdetailsでid_タイトル_パスを持つ() {
    let r = changed_spec();
    let html = r.html();
    let f = sec(&html, 1);
    assert!(f.starts_with(r#"<details class="file" id="d1" open>"#));
    assert!(f.contains("▾"));
    assert!(f.contains("仕様A"));
    assert!(f.contains("DOC-A"));
    assert!(f.contains("docs/spec.md"));
    assert!(f.contains(r#"<span class="st mod">"#));
}

// [FR-116]
#[test]
fn 列見出しと凡例は出さない() {
    let r = changed_spec();
    let html = r.html();
    // no column header row and no legend (body text may contain these words)
    assert!(!html.contains(">変更前<"));
    assert!(!html.contains(">変更後<"));
    assert!(!html.contains("legend"));
    assert!(!html.contains("<thead"));
}

// [FR-116]
#[test]
fn 移動しただけの文書は内容の変更なしの1行だけで折り畳みも行も出ない() {
    let r = repo();
    r.base();
    std::fs::rename(r.root.join("docs/spec.md"), r.root.join("docs/moved.md")).unwrap();
    let html = r.html();
    let f = sec(&html, 1);
    assert!(f.contains("内容の変更なし"));
    assert!(!f.contains(r#"<div class="row"#));
    assert!(!f.contains(r#"<details class="fold">"#));
}

// [FR-116]
#[test]
fn タイトルが変わったときは旧タイトルを取り消し線で並べる() {
    let r = repo();
    r.base();
    r.write("docs/spec.md", &doc("DOC-A", "新題名", "本文です。\n"));
    let html = r.html();
    let f = sec(&html, 1);
    assert!(
        f.contains("<s>仕様A</s>") || f.contains("<del>仕様A</del>"),
        "{f}"
    );
    assert!(f.contains("新題名"));
}

// [FR-116]
#[test]
fn descriptionの変更は本文より前の行として出る() {
    let r = repo();
    r.write(
        "docs/spec.md",
        "---\ntype: BusinessNeed\nid: DOC-A\ntitle: 仕様A\ndescription: 旧要約\n---\n本文です。\n",
    );
    r.base();
    r.write(
        "docs/spec.md",
        "---\ntype: BusinessNeed\nid: DOC-A\ntitle: 仕様A\ndescription: 新要約\n---\n本文です。\n",
    );
    let html = r.html();
    let f = sec(&html, 1);
    let label = f.find("description").expect("description label");
    let plain = strip_spans(f);
    assert!(label < f.find("旧").unwrap());
    assert!(plain.contains("旧要約") && plain.contains("新要約"));
}

// [FR-116]
#[test]
fn 追加された文書は1列で出し削除された文書も1列で出す() {
    let r = repo();
    r.write("docs/gone.md", &doc("DOC-G", "消える", "消える本文。\n"));
    r.base();
    r.rm("docs/gone.md");
    r.write("docs/new.md", &doc("DOC-N", "新規", "新規本文。\n"));
    let html = r.html();
    // tree order: docs/gone.md (d1), docs/new.md (d2)
    let (gone, new) = (sec(&html, 1), sec(&html, 2));
    assert!(gone.contains("消える本文。") && new.contains("新規本文。"));
    for f in [gone, new] {
        assert!(!f.contains(r#"<div class="row mod">"#));
        assert!(!f.contains(r#"<div class="row same">"#));
    }
    assert!(!gone.contains(r#"<div class="row add">"#));
    assert!(!new.contains(r#"<div class="row del">"#));
}

// [FR-116]
#[test]
fn 変わったブロックはrow_modで前後の変わらないブロックはrow_same() {
    let mut base = paras(5);
    let mut feat = base.clone();
    feat[2] = "段落2の本文です。（修正）".to_string();
    base[2] = "段落2の本文です。".to_string();
    let r = spec_edit(&base, &feat);
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row mod">"#), 1);
    // folded rows are still emitted as `row same`
    assert_eq!(count(&html, r#"<div class="row same">"#), 4);
}

// [FR-116]
#[test]
fn 右だけのブロックはrow_add_左だけのブロックはrow_del() {
    let base = vec![
        "共通の最初の段落です。".to_string(),
        "共通の最後の段落です。".to_string(),
    ];
    let mut feat = base.clone();
    feat.insert(1, "zzzzzzzzzzzzzzzzzzzz".to_string());
    let r = spec_edit(&base, &feat);
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row add">"#), 1);
    assert_eq!(count(&html, r#"<div class="row del">"#), 0);

    let r = spec_edit(&feat, &base);
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row del">"#), 1);
    assert_eq!(count(&html, r#"<div class="row add">"#), 0);
}

// [FR-116]
#[test]
fn 画像だけのブロックの差し替えはrow_img() {
    let r = repo();
    r.write("docs/spec.md", &spec("![図](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.base();
    r.write_bin("docs/pic/a.png", &png(2));
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row img">"#), 1, "{html}");
}

// [FR-116]
#[test]
fn 変更量バーは緑4赤1灰0になる() {
    // mod (a=1,d=1) + 3 adds (a=3) -> a=4, d=1 -> 4 green, 1 red
    let base = vec![
        "最初の段落の本文です。".to_string(),
        "固定の段落です。".to_string(),
    ];
    let feat = vec![
        "最初の段落の本文です。（修正）".to_string(),
        "固定の段落です。".to_string(),
        "甲甲甲甲甲甲甲甲".to_string(),
        "乙乙乙乙乙乙乙乙".to_string(),
        "丙丙丙丙丙丙丙丙".to_string(),
    ];
    let r = spec_edit(&base, &feat);
    let html = r.html();
    assert_eq!(bar_counts(sec(&html, 1)), (4, 1, 0));
}

// [FR-116]
#[test]
fn 変更量バーは削除だけなら赤5() {
    let base = vec![
        "削除する段落その一です。".to_string(),
        "削除する段落その二です。".to_string(),
    ];
    let r = spec_edit(&base, &["".to_string()][..0]);
    let html = r.html();
    assert_eq!(bar_counts(sec(&html, 1)), (0, 5, 0));
}

// [FR-116]
#[test]
fn 変更量バーは0でない側を必ず1個以上にして合計5を超えたら多い方から減らす() {
    // a=1, d=20: green round(0.24)=0 -> 1, red round(4.76)=5 -> total 6 -> red 4
    let base: Vec<String> = (0..20).map(|i| format!("line-{i}-xyz")).collect();
    let feat = vec!["日本語の新しい段落です".to_string()];
    let r = spec_edit(&base, &feat);
    let html = r.html();
    assert_eq!(bar_counts(sec(&html, 1)), (1, 4, 0));
}

// [FR-116]
#[test]
fn 移動しただけの文書の変更量バーはすべて灰色() {
    let r = repo();
    r.base();
    std::fs::rename(r.root.join("docs/spec.md"), r.root.join("docs/moved.md")).unwrap();
    let html = r.html();
    assert_eq!(bar_counts(sec(&html, 1)), (0, 0, 5));
}

// ---------------------------------------------------------------- FR-117

// [FR-117]
#[test]
fn 未変更の見出しとページ内リンクは左右で同じブロックになる() {
    let r = repo();
    r.write(
        "docs/spec.md",
        &spec("## 見出し\n\n[見出しへ](#見出し)\n\n旧い本文です。\n"),
    );
    r.base();
    r.write(
        "docs/spec.md",
        &spec("## 見出し\n\n[見出しへ](#見出し)\n\n新しい本文です。\n"),
    );
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row mod">"#), 1);
    assert_eq!(count(&html, r#"<div class="row same">"#), 2);
}

// [FR-117]
// [FR-119]
#[test]
fn 見出しidには文書と側を表す接頭辞が付く() {
    let r = repo();
    r.write("docs/spec.md", &spec("## 旧見出し\n"));
    r.base();
    r.write("docs/spec.md", &spec("## 新見出し\n"));
    let html = r.html();
    assert!(html.contains(r#"id="d1-b-旧見出し""#), "{html}");
    assert!(html.contains(r#"id="d1-h-新見出し""#), "{html}");
}

// [FR-117]
#[test]
fn 文字の強調は変更前をddと変更後をdaのspanで囲む() {
    let r = spec_edit(
        &["これは赤い車です。".to_string()],
        &["これは青い車です。".to_string()],
    );
    let html = r.html();
    assert!(html.contains(r#"<span class="dd">赤</span>"#), "{html}");
    assert!(html.contains(r#"<span class="da">青</span>"#), "{html}");
}

// [FR-117]
#[test]
fn 文字の強調はテキストの内側だけでタグの外に要素を作らない() {
    let r = spec_edit(
        &["これは**赤い**車です。".to_string()],
        &["これは**青い**車です。".to_string()],
    );
    let html = r.html();
    assert!(
        html.contains(r#"<strong><span class="da">青</span>い</strong>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<strong><span class="dd">赤</span>い</strong>"#),
        "{html}"
    );
    assert!(!html.contains(r#"<span class="dd"><"#));
    assert!(!html.contains(r#"<span class="da"><"#));
}

// [FR-117]
#[test]
fn インラインコードの中の文字も囲む() {
    let r = spec_edit(
        &["値は `foo` です。".to_string()],
        &["値は `fob` です。".to_string()],
    );
    let html = r.html();
    assert!(
        html.contains(r#"<code>fo<span class="dd">o</span></code>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<code>fo<span class="da">b</span></code>"#),
        "{html}"
    );
}

// [FR-117]
#[test]
fn 既存の取り消し線は文字の強調と区別される() {
    let r = spec_edit(
        &["~~消す~~ 赤い車".to_string()],
        &["~~消す~~ 青い車".to_string()],
    );
    let html = r.html();
    assert!(html.contains("<del>消す</del>"), "{html}");
    assert_eq!(count(&html, r#"<span class="dd">"#), 1);
    assert_eq!(count(&html, r#"<span class="da">"#), 1);
}

// [FR-117]
#[test]
fn リンク先だけ変わったブロックは文字の強調なしの変更になる() {
    let r = repo();
    r.write("docs/other.md", &doc("DOC-O", "他1", "o\n"));
    r.write("docs/other2.md", &doc("DOC-O2", "他2", "o2\n"));
    r.write("docs/spec.md", &spec("[他](other.md)\n"));
    r.base();
    r.write("docs/spec.md", &spec("[他](other2.md)\n"));
    let html = r.html_with(&["--link-base", "https://e.com/"]);
    assert_eq!(count(&html, r#"<div class="row mod">"#), 1);
    assert!(!html.contains(r#"<span class="dd">"#));
    assert!(!html.contains(r#"<span class="da">"#));
}

// [FR-117]
#[test]
fn 画像だけのブロックの参照先の変更は左右の対になる() {
    let r = repo();
    r.write("docs/spec.md", &spec("![図](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.write_bin("docs/pic/b.png", &png(2));
    r.base();
    r.write("docs/spec.md", &spec("![図](pic/b.png)\n"));
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row img">"#), 1);
    assert!(!html.contains(r#"<div class="row add">"#));
    assert!(!html.contains(r#"<div class="row del">"#));
}

// [FR-117]
#[test]
fn 脚注はラベルで表示され接頭辞付きidを持ち左右がそれぞれ自分の定義を指す() {
    let r = repo();
    r.write(
        "docs/spec.md",
        &spec("参照あり[^note]。\n\n[^note]: 旧い定義です\n"),
    );
    r.base();
    r.write(
        "docs/spec.md",
        &spec("参照あり[^note]。\n\n[^note]: 新しい定義です\n"),
    );
    let html = r.html();
    assert!(html.contains(r#"id="d1-h-fn-note""#), "{html}");
    assert!(html.contains(r#"id="d1-b-fn-note""#), "{html}");
    assert!(html.contains("#d1-h-fn-note"));
    assert!(html.contains("#d1-b-fn-note"));
    assert!(html.contains(">note<"), "label shown, not a number");
}

// [FR-117]
#[test]
fn 脚注のidは文書間で衝突しない() {
    let r = repo();
    r.write(
        "docs/a.md",
        &doc("DOC-1", "一", "x[^note]\n\n[^note]: 旧A\n"),
    );
    r.write(
        "docs/b.md",
        &doc("DOC-2", "二", "y[^note]\n\n[^note]: 旧B\n"),
    );
    r.base();
    r.write(
        "docs/a.md",
        &doc("DOC-1", "一", "x[^note]\n\n[^note]: 新A\n"),
    );
    r.write(
        "docs/b.md",
        &doc("DOC-2", "二", "y[^note]\n\n[^note]: 新B\n"),
    );
    let html = r.html();
    for id in [
        "d1-h-fn-note",
        "d1-b-fn-note",
        "d2-h-fn-note",
        "d2-b-fn-note",
    ] {
        assert_eq!(count(&html, &format!("id=\"{id}\"")), 1, "{id}");
    }
}

// ---------------------------------------------------------------- FR-118

// [FR-118]
#[test]
fn 変わったブロックの前後1ブロックだけ表示し残りを畳む() {
    let base = paras(10);
    let mut feat = base.clone();
    feat[5] = "段落5の本文です。（修正）".to_string();
    let r = spec_edit(&base, &feat);
    let html = r.html();
    let f = sec(&html, 1);
    assert_eq!(count(f, r#"<details class="fold">"#), 2);
    assert!(f.contains("↕ 4"), "{f}");
    assert!(f.contains("↕ 3"), "{f}");
    assert!(!f.contains("§"));
    assert_eq!(count(f, r#"<div class="row same">"#), 2 + 4 + 3);
}

// [FR-118]
#[test]
fn 畳んだ範囲の見出しは最後の見出しで見出しが無ければ直近の前の見出し() {
    let mut base = vec![
        "## 前書き".to_string(),
        "前書きの一です。".to_string(),
        "前書きの二です。".to_string(),
        "## 設計方針".to_string(),
        "方針の一です。".to_string(),
        "方針の二です。".to_string(),
        "方針の三です。".to_string(),
        "変更される段落です。".to_string(),
        "方針の四です。".to_string(),
        "方針の五です。".to_string(),
    ];
    let mut feat = base.clone();
    feat[7] = "変更された段落です。".to_string();
    base[7] = "変更される段落です。".to_string();
    let r = spec_edit(&base, &feat);
    let html = r.html();
    let f = sec(&html, 1);
    assert!(f.contains("↕ 6 § 設計方針"), "{f}");
    assert!(f.contains("↕ 1 § 設計方針"), "{f}");
}

// [FR-118]
#[test]
fn 変わったブロックが近いときは間を畳まず離れていれば畳む() {
    let base = paras(7);
    // changes at 1 and 5: block 3 is the only hidden one
    let mut feat = base.clone();
    feat[1] = "段落1の本文です。（修正）".to_string();
    feat[5] = "段落5の本文です。（修正）".to_string();
    let html = spec_edit(&base, &feat).html();
    assert_eq!(count(sec(&html, 1), r#"<details class="fold">"#), 1);
    assert!(sec(&html, 1).contains("↕ 1"));

    // changes at 1 and 4: blocks 2 and 3 are both adjacent to a change
    let mut feat = base.clone();
    feat[1] = "段落1の本文です。（修正）".to_string();
    feat[4] = "段落4の本文です。（修正）".to_string();
    let html = spec_edit(&base, &feat).html();
    // block 6 is two away from the nearest change: a trailing fold
    assert_eq!(count(sec(&html, 1), r#"<details class="fold">"#), 1);
}

// [FR-118]
#[test]
fn 先頭のブロックが変わったときは手前に折り畳みを出さない() {
    let base = paras(6);
    let mut feat = base.clone();
    feat[0] = "段落0の本文です。（修正）".to_string();
    let html = spec_edit(&base, &feat).html();
    let f = sec(&html, 1);
    assert_eq!(count(f, r#"<details class="fold">"#), 1);
    assert!(
        f.find(r#"<details class="fold">"#).unwrap() > f.find(r#"<div class="row mod">"#).unwrap()
    );
}

// [FR-118]
#[test]
fn 移動のみの文書と追加された文書には折り畳みを出さない() {
    let r = repo();
    r.write("docs/spec.md", &spec(&join(&paras(10))));
    r.base();
    std::fs::rename(r.root.join("docs/spec.md"), r.root.join("docs/moved.md")).unwrap();
    r.write("docs/new.md", &doc("DOC-N", "新規", &join(&paras(10))));
    let html = r.html();
    assert!(!html.contains(r#"<details class="fold">"#));
}

// ---------------------------------------------------------------- FR-119

fn link_repo() -> Repo {
    let r = repo();
    r.write("docs/other.md", &doc("DOC-O", "他", "他の本文。\n"));
    r.write(
        "docs/日本語.md",
        &doc("DOC-J", "日本語", "日本語の本文。\n"),
    );
    r.write("req/fr1.md", &req("FR-001", "要件1", "要件本文。\n"));
    r.write("docs/spec.md", &spec("旧い本文。\n"));
    r.base();
    r
}

const LB: &str = "https://github.com/o/r/blob/abc";

// [FR-119]
#[test]
fn http_https_mailtoはそのまま残る() {
    let r = link_repo();
    r.write(
        "docs/spec.md",
        &spec("[a](https://example.com/x) [b](http://example.com) [c](mailto:a@b.c)\n"),
    );
    let html = r.html();
    for h in [
        "https://example.com/x",
        "http://example.com",
        "mailto:a@b.c",
    ] {
        assert!(html.contains(&format!("href=\"{h}\"")), "{h}");
    }
}

// [FR-119]
#[test]
fn 差分に含まれる文書へのリンクはアンカーになりフラグメントは同じ側の見出しid() {
    let r = repo();
    r.write(
        "docs/spec.md",
        &spec("[要件](../req/fr1.md) [節](../req/fr1.md#sec) 旧い本文。\n"),
    );
    r.base();
    r.write(
        "docs/spec.md",
        &spec("[要件](../req/fr1.md) [節](../req/fr1.md#sec) 新しい本文。\n"),
    );
    r.write("req/fr1.md", &req("FR-001", "要件1", "変更。\n"));
    let html = r.html();
    // docs/spec.md is d1, req/fr1.md is d2
    assert!(html.contains(r##"href="#d2""##), "{html}");
    assert!(html.contains(r##"href="#d2-b-sec""##), "{html}");
    assert!(html.contains(r##"href="#d2-h-sec""##), "{html}");
}

// [FR-119]
#[test]
fn 差分に含まれない文書へのリンクはlink_baseとリポジトリ相対パスをつなぐ() {
    let r = link_repo();
    r.write(
        "docs/spec.md",
        &spec("[他](other.md) [節](other.md#x) 新しい本文。\n"),
    );
    let html = r.html_with(&["--link-base", LB]);
    assert!(
        html.contains(&format!("href=\"{LB}/docs/other.md\"")),
        "{html}"
    );
    assert!(
        html.contains(&format!("href=\"{LB}/docs/other.md#x\"")),
        "{html}"
    );
}

// [FR-119]
#[test]
fn link_baseの末尾スラッシュは補われ有っても同じ() {
    let r = link_repo();
    r.write("docs/spec.md", &spec("[他](other.md) 新しい本文。\n"));
    let a = r.html_with(&["--link-base", LB]);
    let b = r.html_with_second(&["--link-base", &format!("{LB}/")]);
    assert!(a.contains(&format!("href=\"{LB}/docs/other.md\"")));
    assert!(b.contains(&format!("href=\"{LB}/docs/other.md\"")));
}

// [FR-119]
#[test]
fn リンク先のパスはsiteと同じ規則でパーセントエンコードされる() {
    let r = link_repo();
    r.write("docs/spec.md", &spec("[日](日本語.md) 新しい本文。\n"));
    let html = r.html_with(&["--link-base", LB]);
    assert!(
        html.contains(&format!(
            "href=\"{LB}/docs/%E6%97%A5%E6%9C%AC%E8%AA%9E.md\""
        )),
        "{html}"
    );
}

// [FR-119]
#[test]
fn link_baseが無いときと公開されないリンクは文字だけを残す() {
    let r = link_repo();
    r.write(
        "docs/spec.md",
        &spec("[他文書](other.md) [js](javascript:alert(1)) [外](../../outside.md) [読む](../README.md) 新しい本文。\n"),
    );
    let o = r.diff(&["--out", &r.out("n.html")]);
    assert_eq!(o.code, 0);
    assert_eq!(o.stderr, "");
    let html = std::fs::read_to_string(r.out("n.html")).unwrap();
    for t in ["他文書", "js", "外", "読む"] {
        assert!(html.contains(t), "{t}");
    }
    for h in ["other.md", "javascript:", "outside.md", "README.md"] {
        assert!(!html.contains(h), "{h}");
    }
    let linked = r.html_with_second(&["--link-base", LB]);
    assert!(!linked.contains("javascript:"));
    assert!(!linked.contains("outside.md"));
    assert!(!linked.contains("README.md"));
    assert!(linked.contains(&format!("href=\"{LB}/docs/other.md\"")));
}

// [FR-119]
#[test]
fn link_baseはhttpかhttpsで始まる値だけ受け付けそれ以外はexit_1() {
    let r = changed_spec();
    for bad in [
        "ftp://e.com/",
        "e.com/x",
        "javascript:alert(1)",
        "/abs/path",
        "",
    ] {
        r.expect_error(&["--link-base", bad]);
    }
    for good in ["http://e.com/x", "https://e.com/x/"] {
        let out = r.out("ok.html");
        let _ = std::fs::remove_file(&out);
        assert_eq!(
            r.diff(&["--out", &out, "--link-base", good]).code,
            0,
            "{good}"
        );
    }
}

// ---------------------------------------------------------------- FR-120

// [FR-120]
#[test]
fn 条件を満たす画像はdata_uriで埋め込まれ参照されない画像は埋め込まれない() {
    let r = repo();
    r.write_bin("docs/pic/a.png", &png(1));
    r.write_bin("docs/pic/unused.png", &png(9));
    r.base();
    r.write("docs/spec.md", &spec("![図A](pic/a.png)\n"));
    let html = r.html();
    assert!(html.contains(&data_uri(&png(1))), "{html}");
    assert!(!html.contains(&b64(&png(9))));
}

// [FR-120]
#[test]
fn 変更前の画像は変更前の時点から変更後の画像は作業ツリーから読む() {
    let r = repo();
    r.write("docs/spec.md", &spec("![図](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.base();
    r.write_bin("docs/pic/a.png", &png(2));
    common::git_commit(&r.root, "swap");
    r.write_bin("docs/pic/a.png", &png(3)); // uncommitted
    let html = r.html();
    assert!(html.contains(&data_uri(&png(1))));
    assert!(html.contains(&data_uri(&png(3))));
    assert!(!html.contains(&b64(&png(2))));
}

// [FR-120]
#[test]
fn https画像はそのまま参照しsvgや条件を満たさない画像はimgを出さず代替テキストだけ残す() {
    let r = repo();
    r.write_bin("assets/outside.png", &png(5));
    r.base();
    r.write(
        "docs/spec.md",
        &spec("![HTTPSALT](https://img.example.com/a.png)\n\n![SVGALT](a.svg)\n\n![MISSALT](gone.png)\n\n![OUTALT](../assets/outside.png)\n"),
    );
    let o = r.diff(&["--out", &r.out("i.html")]);
    assert_eq!(o.code, 0);
    assert_eq!(o.stderr, "");
    let html = std::fs::read_to_string(r.out("i.html")).unwrap();
    assert!(
        html.contains(r#"src="https://img.example.com/a.png""#),
        "{html}"
    );
    assert_eq!(count(&html, "<img"), 1, "{html}");
    for t in ["SVGALT", "MISSALT", "OUTALT"] {
        assert!(html.contains(t), "{t}");
    }
    assert!(!html.contains("data:image/svg"));
    assert!(!html.contains(&b64(&png(5))));
}

// [FR-120]
#[test]
fn 同じパスで内容が違う画像だけのブロックは左右の対になる() {
    let r = repo();
    r.write("docs/spec.md", &spec("![](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.base();
    r.write_bin("docs/pic/a.png", &png(2));
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row img">"#), 1);
    assert!(html.contains(&data_uri(&png(1))));
    assert!(html.contains(&data_uri(&png(2))));
}

// [FR-120]
#[test]
fn 画像の参照先が違う画像だけのブロックも左右の対になる() {
    let r = repo();
    r.write("docs/spec.md", &spec("![](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.write_bin("docs/pic/b.png", &png(2));
    r.base();
    r.write("docs/spec.md", &spec("![](pic/b.png)\n"));
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row img">"#), 1);
}

// [FR-117]
#[test]
fn 開始番号付きの入れ子リストは変わった項目だけがrow_modになり番号が保たれる() {
    let r = repo();
    r.write(
        "docs/spec.md",
        &spec("3. 三番目の項目です\n   - 入れ子の甲\n   - 入れ子の乙\n4. 四番目の項目です\n5. 五番目の項目です\n"),
    );
    r.base();
    r.write(
        "docs/spec.md",
        &spec("3. 三番目の項目です\n   - 入れ子の甲\n   - 入れ子の乙\n4. 四番目の項目です（修正）\n5. 五番目の項目です\n"),
    );
    let html = r.html();
    assert_eq!(count(&html, r#"<div class="row mod">"#), 1, "{html}");
    assert_eq!(count(&html, r#"<div class="row same">"#), 2, "{html}");
    // each item block keeps its own number
    assert!(html.contains(r#"<ol start="3">"#), "{html}");
    assert!(html.contains(r#"<ol start="4">"#), "{html}");
    assert!(html.contains(r#"<ol start="5">"#), "{html}");
    assert!(html.contains("入れ子の乙"));
}

// [FR-120]
#[cfg(unix)]
#[test]
fn 読めない画像があるとexit_1で何も書かずパスをstderrに出す() {
    use std::os::unix::fs::PermissionsExt;
    let r = repo();
    r.write("docs/spec.md", &spec("![図](pic/a.png)\n"));
    r.write_bin("docs/pic/a.png", &png(1));
    r.base();
    r.write("docs/spec.md", &spec("![図](pic/a.png) 変更\n"));
    let img = r.root.join("docs/pic/a.png");
    std::fs::set_permissions(&img, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&img).is_ok() {
        // running as root: permissions do not block reads
        return;
    }
    let e = r.expect_error(&[]);
    assert!(e.contains("docs/pic/a.png"), "{e}");
    std::fs::set_permissions(&img, std::fs::Permissions::from_mode(0o644)).unwrap();
}

// [FR-119]
#[test]
fn 移動しただけの文書へのリンクはフラグメント付きでも枠のアンカーになる() {
    let r = repo();
    r.write(
        "docs/a.md",
        &doc("DOC-L", "リンク元", "[節](target.md#section) 本文。\n"),
    );
    r.write(
        "docs/target.md",
        &doc("DOC-T", "移動先", "## section\n\n中身。\n"),
    );
    r.base();
    std::fs::create_dir_all(r.root.join("docs/sub")).unwrap();
    std::fs::rename(
        r.root.join("docs/target.md"),
        r.root.join("docs/sub/target.md"),
    )
    .unwrap();
    r.write(
        "docs/a.md",
        &doc(
            "DOC-L",
            "リンク元",
            "[節](sub/target.md#section) 本文を直した。\n",
        ),
    );
    let html = r.html();
    // docs/sub/target.md is d1 (moved only), docs/a.md is d2
    assert!(sec(&html, 1).contains("内容の変更なし"), "{html}");
    assert!(sec(&html, 2).contains(r##"href="#d1""##), "{html}");
    assert!(!html.contains(r##"href="#d1-"##), "{html}");
}

// [FR-112]
#[test]
fn 公開されないid無しreqは画像の差し替えだけでもid無しエラーになる() {
    let r = repo();
    r.write(
        "req/noid.md",
        "---\ntype: Requirement\ntitle: 無し\n---\n![図](img.png)\n",
    );
    r.write_bin("req/img.png", &png(1));
    r.base();
    r.write_bin("req/img.png", &png(2));
    let e = r.expect_error(&[]);
    assert!(e.contains("req/noid.md"), "{e}");
}

// [FR-116]
// [FR-119]
#[test]
fn 表示が同じでソースだけ変わった移動文書は通常の枠で畳まれ見出しidへのリンクが有効() {
    let r = repo();
    r.write(
        "docs/a.md",
        &doc("DOC-L", "リンク元", "[節](target.md#section) 旧。\n"),
    );
    r.write(
        "docs/target.md",
        &doc("DOC-T", "移動先", "## section\n\n*強調* です。\n"),
    );
    r.base();
    std::fs::create_dir_all(r.root.join("docs/sub")).unwrap();
    std::fs::rename(
        r.root.join("docs/target.md"),
        r.root.join("docs/sub/target.md"),
    )
    .unwrap();
    // only the source differs (`*x*` -> `_x_`); it renders the same
    r.write(
        "docs/sub/target.md",
        &doc("DOC-T", "移動先", "## section\n\n_強調_ です。\n"),
    );
    r.write(
        "docs/a.md",
        &doc("DOC-L", "リンク元", "[節](sub/target.md#section) 新。\n"),
    );
    let html = r.html();
    // docs/sub/target.md is d1, docs/a.md is d2
    let moved = sec(&html, 1);
    assert!(!moved.contains("内容の変更なし"), "{moved}");
    assert!(moved.contains(r#"<details class="fold">"#), "{moved}");
    assert!(html.contains(r#"id="d1-h-section""#), "{html}");
    assert!(
        sec(&html, 2).contains(r##"href="#d1-h-section""##),
        "{html}"
    );
}
