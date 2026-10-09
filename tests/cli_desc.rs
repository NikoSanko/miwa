mod common;

use std::path::Path;

const DOC_BODY: &str = "# Spec\n\nSee [req](../req/fr1.md).\n";

fn doc_source(description: &str, body: &str) -> String {
    format!("---\ntype: BusinessNeed\nid: DOC-A\ndescription: {description}\n---\n{body}")
}

/// `index: [docs]` + one Doc (with a description) linking to one req, and one test.
fn write_desc_fixture(dir: &Path) {
    common::write_minimal_fixture(dir);
    common::write(
        dir,
        ".miwa/config.yaml",
        "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\nindex:\n  - docs\ndiff_base: main\njudge:\n  threshold: 0.9\n",
    );
    common::write(dir, "docs/spec.md", &doc_source("要約A", DOC_BODY));
}

fn check_stdout(dir: &Path) -> String {
    let output = common::run(dir, &["check"]).output().unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn has_line(stdout: &str, prefix: &str) -> bool {
    stdout.lines().any(|l| l.starts_with(prefix))
}

// [FR-086]
#[test]
fn index設定があるとcheckはdescエッジをnewとして報告する() {
    let dir = tempfile::tempdir().unwrap();
    write_desc_fixture(dir.path());
    let stdout = check_stdout(dir.path());
    assert!(
        has_line(&stdout, "new Edge desc:doc:DOC-A NewEdge"),
        "{stdout}"
    );
    assert!(
        has_line(&stdout, "new Edge doc-req:DOC-A:FR-001 NewEdge"),
        "{stdout}"
    );
}

// [FR-086]
#[test]
fn index設定が無ければcheckはdescエッジを報告しない() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());
    let stdout = check_stdout(dir.path());
    assert!(!stdout.contains("desc:"), "{stdout}");
}

// [FR-086]
#[test]
fn reconcile後のcheckはdescエッジのfindingを出さない() {
    let dir = tempfile::tempdir().unwrap();
    write_desc_fixture(dir.path());
    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    let stdout = check_stdout(dir.path());
    assert!(!stdout.contains("desc:doc:DOC-A"), "{stdout}");
}

// [FR-088]
#[test]
fn 要約を変えるとdescエッジだけがdescription_changedのdriftになる() {
    let dir = tempfile::tempdir().unwrap();
    write_desc_fixture(dir.path());
    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    common::write(
        dir.path(),
        "docs/spec.md",
        &doc_source("変更後の要約", DOC_BODY),
    );
    let stdout = check_stdout(dir.path());
    assert!(
        has_line(&stdout, "drift Edge desc:doc:DOC-A DescriptionChanged"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("drift Edge doc-req:"),
        "要約の変更で doc-req は drift しない: {stdout}"
    );
    assert!(!stdout.contains("drift Edge test-req:"), "{stdout}");
}

// [FR-088]
#[test]
fn doc本文を変えるとdoc_reqのdoc_changedとdescのbody_changedが別々に出る() {
    let dir = tempfile::tempdir().unwrap();
    write_desc_fixture(dir.path());
    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    common::write(
        dir.path(),
        "docs/spec.md",
        &doc_source("要約A", &format!("{DOC_BODY}\n追記した本文。\n")),
    );
    let stdout = check_stdout(dir.path());
    assert!(
        has_line(&stdout, "drift Edge doc-req:DOC-A:FR-001 DocChanged"),
        "{stdout}"
    );
    assert!(
        has_line(&stdout, "drift Edge desc:doc:DOC-A BodyChanged"),
        "{stdout}"
    );
}

// [FR-088]
#[test]
fn 要約を消すとindexのmissing_descriptionとdescのdriftの2件が出る() {
    let dir = tempfile::tempdir().unwrap();
    write_desc_fixture(dir.path());
    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    common::write(
        dir.path(),
        "docs/spec.md",
        &format!("---\ntype: BusinessNeed\nid: DOC-A\n---\n{DOC_BODY}"),
    );
    let stdout = check_stdout(dir.path());
    assert!(
        has_line(&stdout, "drift Edge desc:doc:DOC-A DescriptionChanged"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("index ") && l.contains("MissingDescription")),
        "{stdout}"
    );
}

// [FR-086]
#[test]
fn diffは基準側にindexが無ければdescエッジを新規のnewとして報告する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    common::write_minimal_fixture(root);
    common::write(root, "docs/spec.md", &doc_source("要約A", DOC_BODY));
    common::run(root, &["reconcile", "--all"])
        .assert()
        .success();
    common::git_commit(root, "base without index");

    common::git_checkout_new_branch(root, "feature");
    write_desc_fixture(root);
    common::git_commit(root, "enable index");

    let output = common::run(
        root,
        &["check", "--diff", "--base", "main", "--format", "json"],
    )
    .output()
    .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings = value["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["kind"] == "New" && f["target"] == "desc:doc:DOC-A"),
        "{findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|f| f["target"] == "doc-req:DOC-A:FR-001"),
        "{findings:?}"
    );
}
