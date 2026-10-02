mod common;

use std::path::Path;

const CONFIG_WITH_INDEX: &str = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\nindex:\n  - docs\n";

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p.to_string_lossy().into_owned());
        }
    }
    out
}

fn stdout_of(dir: &Path, args: &[&str]) -> String {
    let out = common::run(dir, args).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

fn findings(dir: &Path, extra: &[&str]) -> Vec<serde_json::Value> {
    let mut args = vec!["check", "--format", "json"];
    args.extend_from_slice(extra);
    let out = common::run(dir, &args).output().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    value["findings"].as_array().unwrap().clone()
}

fn index_findings(dir: &Path) -> Vec<serde_json::Value> {
    findings(dir, &[])
        .into_iter()
        .filter(|f| f["kind"] == "Index")
        .collect()
}

// [FR-084]
#[test]
fn 索引がなければ平坦なリストで新規作成されcreatedが出力される() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/b.md", &common::typed("B", None));
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));

    let out = stdout_of(root, &["index"]);
    assert_eq!(out, "created docs/index.md\n");
    assert_eq!(
        read(root, "docs/index.md"),
        "- [A](a.md) — DA\n- [B](b.md)\n"
    );
}

// [FR-084]
#[test]
fn 文書が0件の根では索引を作らない() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/untyped.md", "plain\n");

    let out = stdout_of(root, &["index"]);
    assert_eq!(out, "");
    assert!(!root.join("docs/index.md").exists());
}

// [FR-085]
#[test]
fn 更新はupdatedで出力され_2回目は全件unchangedで一時ファイルも残らない() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/sub/b.md", &common::typed("B", Some("DB")));
    common::write(root, "docs/sub/index.md", "- [Old](b.md) — x\n");
    common::write(root, "docs/index.md", "- [Old](a.md) — x\n");

    let first = stdout_of(root, &["index"]);
    assert!(first.contains("updated docs/index.md"), "{first}");
    assert!(first.contains("updated docs/sub/index.md"), "{first}");
    assert_eq!(first.lines().count(), 2, "{first}");
    assert!(read(root, "docs/sub/index.md").starts_with("- [B](b.md) — DB\n"));

    let second = stdout_of(root, &["index"]);
    assert!(second.contains("unchanged docs/index.md"), "{second}");
    assert!(second.contains("unchanged docs/sub/index.md"), "{second}");
    assert_eq!(second.lines().count(), 2, "{second}");
    assert!(!second.contains("updated") && !second.contains("created"));

    for entry in walk(&root.join("docs")) {
        assert!(!entry.ends_with(".tmp"), "leftover temp file: {entry}");
    }
}

// [FR-085]
#[test]
fn indexが未設定ならexit_code_1とメッセージで失敗する() {
    let dir = tempfile::tempdir().unwrap();
    common::write(
        dir.path(),
        ".miwa/config.yaml",
        common::config_yaml_without_index(),
    );
    common::write(dir.path(), "docs/a.md", &common::typed("A", Some("D")));

    let out = common::run(dir.path(), &["index"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("index is not configured in .miwa/config.yaml"),
        "{stderr}"
    );
    assert!(!dir.path().join("docs/index.md").exists());
}

// [FR-085]
#[test]
fn エントリ行以外のバイト列はbomとcrlfを含めて保存される() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    let original = "\u{feff}# 索引\r\n\r\n前書き  です。\r\n\r\n## 節\r\n\r\n- [Old](a.md) — x\r\n  - 子 [c](c.md)\r\n- **[b](b.md)** — 太字\r\n";
    common::write(root, "docs/index.md", original);

    let out = stdout_of(root, &["index"]);
    assert_eq!(out, "updated docs/index.md\n");
    let want = "\u{feff}# 索引\r\n\r\n前書き  です。\r\n\r\n## 節\r\n\r\n- [A](a.md) — DA\r\n  - 子 [c](c.md)\r\n- **[b](b.md)** — 太字\r\n";
    assert_eq!(read(root, "docs/index.md"), want);
}

// [FR-083]
#[test]
fn 存在しない文書のエントリは削除され_除外配下は残る() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(
        root,
        ".miwa/config.yaml",
        &format!("{CONFIG_WITH_INDEX}exclude:\n  - vendor\n"),
    );
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/vendor/v.md", &common::typed("V", Some("DV")));
    common::write(
        root,
        "docs/index.md",
        "- [A](a.md) — DA\n- [G](gone.md) — g\n- [V](vendor/v.md) — DV\n",
    );

    stdout_of(root, &["index"]);
    assert_eq!(
        read(root, "docs/index.md"),
        "- [A](a.md) — DA\n- [V](vendor/v.md) — DV\n"
    );
}

// [FR-082]
#[test]
fn 未列挙の文書と辿れない子の索引が末尾に追加される() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/sub/b.md", &common::typed("B", Some("DB")));
    common::write(root, "docs/sub/index.md", "- [B](b.md) — DB\n");
    common::write(root, "docs/index.md", "# 索引\n\n前書き\n");

    stdout_of(root, &["index"]);
    assert_eq!(
        read(root, "docs/index.md"),
        "# 索引\n\n前書き\n\n- [A](a.md) — DA\n- [sub](sub/) — \n"
    );
}

// [FR-085]
#[test]
fn index実行後に残るindexのfindingは人の判断が要るものだけになる() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/nodesc.md", &common::typed("N", None));
    common::write(root, "docs/untyped.md", "plain\n");
    common::write(root, "docs/sub/b.md", &common::typed("B", Some("DB")));
    common::write(root, "docs/sub/index.md", "intro\n");

    stdout_of(root, &["index"]);
    let allowed = [
        "MissingDescription",
        "MissingDirectoryDescription",
        "UntypedFile",
        "BrokenEntry",
    ];
    let found = index_findings(root);
    assert!(!found.is_empty());
    for f in &found {
        let reason = f["reason"].as_str().unwrap();
        assert!(allowed.contains(&reason), "unexpected {f}");
    }
    assert!(found
        .iter()
        .any(|f| f["reason"] == "MissingDescription" && f["target"] == "docs/nodesc.md"));
}

// [FR-073]
#[test]
fn 根の索引がなければcheckがmissing_indexを報告する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));

    let found = index_findings(root);
    assert!(
        found.iter().any(|f| f["reason"] == "MissingIndex"
            && f["target"] == "docs"
            && f["target_kind"] == "Index"),
        "{found:?}"
    );
}

// [FR-074]
#[test]
fn 列挙されていない文書をcheckがnot_listedで報告する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/index.md", "intro\n");

    let found = index_findings(root);
    assert!(found
        .iter()
        .any(|f| f["reason"] == "NotListed" && f["target"] == "docs/a.md"));
}

// [FR-077]
#[test]
fn タイトルと要約の不一致をcheckが報告する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/index.md", "- [X](a.md) — other\n");

    let found = index_findings(root);
    assert!(found
        .iter()
        .any(|f| f["reason"] == "TitleMismatch" && f["target"] == "docs/a.md"));
    assert!(found
        .iter()
        .any(|f| f["reason"] == "DescriptionMismatch" && f["target"] == "docs/a.md"));
}

// [FR-076]
#[test]
fn 壊れたエントリの正準形targetがtextとjsonに出る() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/index.md", "- [A](a.md) — DA\n- [g](gone.md)\n");

    let found = index_findings(root);
    let broken = found
        .iter()
        .find(|f| f["reason"] == "BrokenEntry")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(broken["target"], "docs/index.md -> gone.md");
    assert_eq!(broken["detail"], "target does not exist");

    let text = stdout_of(root, &["check"]);
    assert!(
        text.contains("index Index docs/index.md -> gone.md BrokenEntry target does not exist"),
        "{text}"
    );
}

// [FR-080]
#[test]
fn checkのtextとjsonは種別をindexと表示しgateはexit_code_2を返す() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));

    let text = stdout_of(root, &["check"]);
    assert!(text.lines().any(|l| l.starts_with("index ")), "{text}");
    assert!(findings(root, &[]).iter().any(|f| f["kind"] == "Index"));
    common::run(root, &["check", "--gate"]).assert().code(2);
}

// [FR-080]
#[test]
fn indexのfindingが残っていてもreconcile_allは成功しロックに影響しない() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write_minimal_fixture(root);
    common::write(
        root,
        ".miwa/config.yaml",
        &format!("{}index:\n  - docs\n", common::config_yaml_without_index()),
    );
    assert!(!index_findings(root).is_empty());

    common::run(root, &["reconcile", "--all"])
        .assert()
        .success();
    let lock: serde_json::Value = serde_json::from_str(&read(root, ".miwa/lock.json")).unwrap();
    let edges = lock["edges"].as_object().unwrap();
    assert!(edges.contains_key("doc-req:DOC-A:FR-001"));
    assert!(edges.keys().all(|k| !k.contains("index")), "{edges:?}");
    // index finding は reconcile 後も残る
    assert!(!index_findings(root).is_empty());
}

// [FR-080]
#[test]
fn diffは基準側の設定にindexがなければ現在側のindex_findingをすべて新規として報告する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    common::write(
        root,
        ".miwa/config.yaml",
        common::config_yaml_without_index(),
    );
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::git_commit(root, "base");

    common::git_checkout_new_branch(root, "feature");
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::git_commit(root, "enable index");

    let out = common::run(
        root,
        &["check", "--diff", "--base", "main", "--format", "json"],
    )
    .output()
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let found: Vec<&serde_json::Value> = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == "Index")
        .collect();
    assert!(
        found
            .iter()
            .any(|f| f["reason"] == "MissingIndex" && f["target"] == "docs"),
        "{found:?}"
    );
}

// [FR-080]
#[test]
fn diffは基準側にも存在するindex_findingを報告しない() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::git_commit(root, "base");

    common::git_checkout_new_branch(root, "feature");
    common::write(root, "docs/other.txt", "unrelated\n");
    common::git_commit(root, "unrelated change");

    let out = common::run(
        root,
        &["check", "--diff", "--base", "main", "--format", "json"],
    )
    .output()
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !value["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["kind"] == "Index"),
        "{value}"
    );
}

// [FR-066]
#[test]
fn 設定違反ではindexもcheckもexit_code_1で止まる() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(
        root,
        ".miwa/config.yaml",
        &format!("{}index:\n  - other\n", common::config_yaml_without_index()),
    );
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    for args in [&["index"][..], &["check"][..]] {
        let out = common::run(root, args).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
    }
    assert!(!root.join("docs/index.md").exists());
}

// [FR-085]
#[test]
fn 実行後は索引と同じ場所にも他の場所にも一時ファイルが残らない() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/sub/b.md", &common::typed("B", Some("DB")));
    stdout_of(root, &["index"]);
    for entry in walk(root) {
        assert!(!entry.ends_with(".tmp"), "leftover temp file: {entry}");
    }
}

// [FR-085]
#[test]
fn utf8として読めない索引は書き換えずにexit_code_1で失敗する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    let bytes: &[u8] = b"- [Old](a.md) \xff x\n";
    std::fs::write(root.join("docs/index.md"), bytes).unwrap();

    let out = common::run(root, &["index"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("docs/index.md"));
    assert_eq!(std::fs::read(root.join("docs/index.md")).unwrap(), bytes);
}

// [FR-084]
#[test]
fn 根の索引の新規作成では文書の後に子索引のディレクトリエントリが入る() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write(root, ".miwa/config.yaml", CONFIG_WITH_INDEX);
    common::write(root, "docs/a.md", &common::typed("A", Some("DA")));
    common::write(root, "docs/sub/b.md", &common::typed("B", Some("DB")));
    common::write(root, "docs/sub/index.md", "- [B](b.md) — DB\n");

    let out = stdout_of(root, &["index"]);
    assert!(out.contains("created docs/index.md"), "{out}");
    assert_eq!(
        read(root, "docs/index.md"),
        "- [A](a.md) — DA\n- [sub](sub/) — \n"
    );
}

// [FR-080]
#[test]
fn indexのfindingが残っていてもreconcile_edgeは成功する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::write_minimal_fixture(root);
    common::write(
        root,
        ".miwa/config.yaml",
        &format!("{}index:\n  - docs\n", common::config_yaml_without_index()),
    );
    assert!(!index_findings(root).is_empty());

    common::run(root, &["reconcile", "--edge", "doc-req:DOC-A:FR-001"])
        .assert()
        .success();
    let lock: serde_json::Value = serde_json::from_str(&read(root, ".miwa/lock.json")).unwrap();
    assert!(lock["edges"]
        .as_object()
        .unwrap()
        .contains_key("doc-req:DOC-A:FR-001"));
}
