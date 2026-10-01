mod common;

/// A richer fixture than `write_minimal_fixture`: 3 docs, 5 reqs, 2 Go tests
/// and 2 Rust tests, with file names deliberately out of sync with the ids
/// they carry (so alphabetical file order != id order), and several docs /
/// reqs left unlinked / untested on purpose. Produces >= 10 findings when no
/// lock exists yet, which makes ordering issues observable.
fn write_rich_fixture(dir: &std::path::Path) {
    common::write(
        dir,
        ".miwa/config.yaml",
        "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n",
    );

    // ファイル名のアルファベット順と id の順が一致しないようにする
    common::write(
        dir,
        "docs/omega.md",
        "---\ntype: Specification\nid: DOC-C\n---\n[a](../req/zulu.md) と [b](../req/echo.md)\n",
    );
    common::write(
        dir,
        "docs/alpha.md",
        "---\ntype: Specification\nid: DOC-A\n---\n[c](../req/mike.md)\n",
    );
    common::write(
        dir,
        "docs/kappa.md",
        "---\ntype: Specification\nid: DOC-B\n---\nどこにもリンクしない。\n",
    );

    common::write(
        dir,
        "req/zulu.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文1。\n",
    );
    common::write(
        dir,
        "req/echo.md",
        "---\ntype: Requirement\nid: FR-002\n---\n本文2。\n",
    );
    common::write(
        dir,
        "req/mike.md",
        "---\ntype: Requirement\nid: FR-003\n---\n本文3。\n",
    );
    common::write(
        dir,
        "req/bravo.md",
        "---\ntype: Requirement\nid: FR-004\n---\n本文4（どの doc からもリンクされない）。\n",
    );
    common::write(
        dir,
        "req/golf.md",
        "---\ntype: Requirement\nid: FR-005\n---\n本文5（どの doc からもリンクされない）。\n",
    );

    common::write(
        dir,
        "tests/sample_test.go",
        "package sample\n\n// [FR-001]\nfunc TestOne(t *testing.T) {}\n\n// [FR-003]\nfunc TestThree(t *testing.T) {}\n",
    );
    common::write(
        dir,
        "tests/other.rs",
        "// [FR-002]\n#[test]\nfn checks_two() {\n    assert!(true);\n}\n\n// [FR-003]\n#[test]\nfn checks_three_again() {\n    assert!(true);\n}\n",
    );
}

// [FR-028]
#[test]
fn checkはロックファイルの内容を書き換えない() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    // まず reconcile --all でロックを作っておく（findings がある状態を作らないため）
    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    let lock_path = dir.path().join(".miwa/lock.json");
    let before = std::fs::read(&lock_path).unwrap();

    common::run(dir.path(), &["check"]).assert().success();

    let after = std::fs::read(&lock_path).unwrap();
    assert_eq!(before, after, "check must not modify the lock file");
}

// [FR-029]
#[test]
fn checkは同じ入力に対して常に同じ出力を返す() {
    let dir = tempfile::tempdir().unwrap();
    write_rich_fixture(dir.path());

    let first_json = common::run(dir.path(), &["check", "--format", "json"])
        .output()
        .unwrap();
    let second_json = common::run(dir.path(), &["check", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(first_json.stdout, second_json.stdout);
    assert_eq!(first_json.status.code(), second_json.status.code());

    let value: serde_json::Value = serde_json::from_slice(&first_json.stdout).unwrap();
    let findings = value["findings"].as_array().unwrap();
    assert!(
        findings.len() >= 10,
        "fixture should produce at least 10 findings to make ordering bugs observable, got {}: {findings:?}",
        findings.len()
    );

    let first_text = common::run(dir.path(), &["check", "--format", "text"])
        .output()
        .unwrap();
    let second_text = common::run(dir.path(), &["check", "--format", "text"])
        .output()
        .unwrap();
    assert_eq!(first_text.stdout, second_text.stdout);
    assert_eq!(first_text.status.code(), second_text.status.code());
}

// [FR-030]
#[test]
fn gateはfindingがあればexit_code_2を返す() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());
    // ロック未作成なので new の finding が存在するはず

    common::run(dir.path(), &["check", "--gate"])
        .assert()
        .code(2);
}

// [FR-030]
#[test]
fn gateなしのcheckはfindingがあってもexit_code_0を返す() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    common::run(dir.path(), &["check"]).assert().code(0);
}

// [FR-030]
#[test]
fn findingが0件でgateがあってもexit_code_0を返す() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());
    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();

    common::run(dir.path(), &["check", "--gate"])
        .assert()
        .code(0);
}

// [FR-030]
#[test]
fn 設定エラーの場合はexit_code_1を返す() {
    let dir = tempfile::tempdir().unwrap();
    // .miwa/config.yaml を用意しない（見つからない）か、不正な値にする
    common::write(
        dir.path(),
        ".miwa/config.yaml",
        "docs: []\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n",
    );

    common::run(dir.path(), &["check", "--gate"])
        .assert()
        .code(1);
}

// [FR-032]
#[test]
fn textとjsonは同じ数のfindingを表す() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    let text_out = common::run(dir.path(), &["check", "--format", "text"])
        .output()
        .unwrap();
    let json_out = common::run(dir.path(), &["check", "--format", "json"])
        .output()
        .unwrap();

    let text = String::from_utf8_lossy(&text_out.stdout);
    let text_lines = text.lines().filter(|l| !l.trim().is_empty()).count();

    let value: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    let json_count = value["findings"].as_array().unwrap().len();

    assert!(json_count > 0, "fixture should produce findings");
    assert_eq!(text_lines, json_count);
}

fn write_lang_fixture(dir: &std::path::Path, test_path: &str, test_src: &str) {
    common::write(dir, ".miwa/config.yaml", "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n");
    common::write(
        dir,
        "docs/spec.md",
        "---\ntype: Specification\nid: DOC-A\n---\n# Spec\n\nSee [req](../req/fr1.md).\n",
    );
    common::write(
        dir,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n最初の要件本文。\n",
    );
    common::write(dir, test_path, test_src);
}

fn check_json(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let out = common::run(dir, &["check", "--format", "json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    value["findings"].as_array().unwrap().clone()
}

fn assert_new_then_drift(test_path: &str, before: &str, after: &str, edge: &str) {
    let dir = tempfile::tempdir().unwrap();
    write_lang_fixture(dir.path(), test_path, before);

    let findings = check_json(dir.path());
    assert!(
        findings
            .iter()
            .any(|f| f["kind"] == "New" && f["target"] == edge),
        "expected new finding for {edge}: {findings:?}"
    );

    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    let findings = check_json(dir.path());
    assert!(
        !findings
            .iter()
            .any(|f| f["kind"] == "New" || f["kind"] == "Drift"),
        "unexpected findings after reconcile: {findings:?}"
    );

    common::write(dir.path(), test_path, after);
    let findings = check_json(dir.path());
    assert!(
        findings
            .iter()
            .any(|f| f["kind"] == "Drift" && f["target"] == edge),
        "expected drift for {edge}: {findings:?}"
    );
}

// [FR-049]
#[test]
fn csharpのtestはnew_reconcile_driftの順で検知される() {
    assert_new_then_drift(
        "tests/AuthTests.cs",
        "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { Assert.True(true); }\n}\n",
        "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { Assert.True(false); }\n}\n",
        "test-req:tests/AuthTests:AuthTests:T:FR-001",
    );
}

// [FR-054]
#[test]
fn typescriptのtestはnew_reconcile_driftの順で検知される() {
    assert_new_then_drift(
        "tests/a.test.ts",
        "describe(\"d\", () => {\n  // [FR-001]\n  it(\"t\", () => { expect(1).toBe(1); });\n});\n",
        "describe(\"d\", () => {\n  // [FR-001]\n  it(\"t\", () => { expect(1).toBe(2); });\n});\n",
        "test-req:tests/a.test:d:t:FR-001",
    );
}

// [FR-057]
#[test]
fn csharpのオーバーロードは1本のedgeにまとまりどれかの本体変更でdriftになる() {
    assert_new_then_drift(
        "tests/AuthTests.cs",
        "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { Assert.True(true); }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { Assert.True(true); }\n}\n",
        "public class AuthTests\n{\n    // [FR-001]\n    [Fact]\n    public void T() { Assert.True(true); }\n\n    // [FR-001]\n    [Fact]\n    public void T(int x) { Assert.True(false); }\n}\n",
        "test-req:tests/AuthTests:AuthTests:T:FR-001",
    );
}
