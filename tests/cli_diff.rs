mod common;

fn write_base_fixture(dir: &std::path::Path) {
    common::write(
        dir,
        ".miwa/config.yaml",
        "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\n",
    );
    common::write(
        dir,
        "docs/spec.md",
        "---\ntype: Specification\nid: DOC-A\n---\n[a](../req/fr1.md) と [b](../req/fr2.md)\n",
    );
    common::write(
        dir,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文1。\n",
    );
    common::write(
        dir,
        "req/fr2.md",
        "---\ntype: Requirement\nid: FR-002\n---\n本文2。\n",
    );
}

// [FR-031]
#[test]
fn diffはmerge_base以降に新たに発生したfindingだけを報告する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    write_base_fixture(root);
    common::run(root, &["reconcile", "--all"])
        .assert()
        .success();
    common::git_commit(root, "base state");

    common::git_checkout_new_branch(root, "feature");
    // req 側だけ変更する（doc は追随していない）→ ReqChanged の drift が新たに発生する
    common::write(
        root,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文1（変更後）。\n",
    );
    common::git_commit(root, "drift fr1");

    let output = common::run(
        root,
        &["check", "--diff", "--base", "main", "--format", "json"],
    )
    .output()
    .unwrap();
    assert!(output.status.success() || output.status.code() == Some(0));

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings = value["findings"].as_array().unwrap();

    // no-test（FR-001, FR-002 いずれも）は merge-base 時点にも既に存在するため diff からは除外される
    assert!(
        !findings.iter().any(|f| f["kind"] == "NoTest"),
        "pre-existing no-test findings should not appear in the diff: {findings:?}"
    );
    // 新たに発生した drift（req 側の変更）だけが報告される
    assert!(findings.iter().any(|f| f["kind"] == "Drift"
        && f["target"] == "doc-req:DOC-A:FR-001"
        && f["reason"] == "ReqChanged"));
}

// [FR-031]
#[test]
fn diffはhead時点のロックで評価する_headで再合意すれば新たなfindingは出ない() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    write_base_fixture(root);
    common::run(root, &["reconcile", "--all"])
        .assert()
        .success();
    common::git_commit(root, "base state");

    common::git_checkout_new_branch(root, "feature");
    // req 側を変更するが、head 上でその場で reconcile --edge して合意してからコミットする
    common::write(
        root,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文1（変更後）。\n",
    );
    common::run(root, &["reconcile", "--edge", "doc-req:DOC-A:FR-001"])
        .assert()
        .success();
    common::git_commit(root, "drift fr1 and reconcile at head");

    let output = common::run(
        root,
        &["check", "--diff", "--base", "main", "--format", "json"],
    )
    .output()
    .unwrap();
    assert!(output.status.success() || output.status.code() == Some(0));

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings = value["findings"].as_array().unwrap();

    // head 自身のロック（reconcile 済み）で評価されるので、drift は起きず diff は空になる。
    // もし実装が base のロックを使って head を評価してしまうと、ここで ReqChanged の drift が現れる。
    assert!(
        findings.is_empty(),
        "expected no new findings when head has re-reconciled: {findings:?}"
    );
}

// [FR-031]
#[test]
fn diffはhead時点の設定で評価する_新しいrequirements領域のreqはnewとno_testになる() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    write_base_fixture(root);
    common::run(root, &["reconcile", "--all"])
        .assert()
        .success();
    common::git_commit(root, "base state");

    common::git_checkout_new_branch(root, "feature");
    // head だけ requirements 領域を追加し、そこに新しい req と、それにリンクする doc を追加する
    common::write(
        root,
        ".miwa/config.yaml",
        "docs:\n  - docs\nrequirements:\n  - req\n  - req2\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\n",
    );
    common::write(
        root,
        "req2/fr3.md",
        "---\ntype: Requirement\nid: FR-003\n---\n本文3。\n",
    );
    common::write(
        root,
        "docs/spec2.md",
        "---\ntype: Specification\nid: DOC-B\n---\n[c](../req2/fr3.md)\n",
    );
    common::git_commit(root, "add req2 area at head");

    let output = common::run(
        root,
        &["check", "--diff", "--base", "main", "--format", "json"],
    )
    .output()
    .unwrap();
    assert!(output.status.success() || output.status.code() == Some(0));

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let findings = value["findings"].as_array().unwrap();

    // base 時点の設定には req2 が含まれていないため、これらは base には存在せず新たな finding として現れる
    assert!(findings
        .iter()
        .any(|f| f["kind"] == "New" && f["target"] == "doc-req:DOC-B:FR-003"));
    assert!(findings
        .iter()
        .any(|f| f["kind"] == "NoTest" && f["target"] == "FR-003"));
}

// [FR-031]
#[test]
fn merge_baseが取得できない場合はexit_code_1で終了する() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    common::git_init(root);
    write_base_fixture(root);
    common::run(root, &["reconcile", "--all"])
        .assert()
        .success();
    common::git_commit(root, "base state");

    common::run(root, &["check", "--diff", "--base", "no-such-branch"])
        .assert()
        .failure()
        .code(1);
}
