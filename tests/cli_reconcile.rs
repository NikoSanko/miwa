mod common;

// [FR-033]
#[test]
fn edge指定のreconcileはロックファイルに現在のハッシュを書き込む() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    common::run(
        dir.path(),
        &[
            "reconcile",
            "--edge",
            "doc-req:DOC-A:FR-001",
            "--reason",
            "最小長の記述が一致",
        ],
    )
    .assert()
    .success();

    let lock_path = dir.path().join(".miwa/lock.json");
    assert!(lock_path.exists());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    let entry = &value["edges"]["doc-req:DOC-A:FR-001"];
    assert_eq!(entry["by"], serde_json::json!("human"));
    assert_eq!(entry["reason"], serde_json::json!("最小長の記述が一致"));
    assert!(entry["from"].as_str().unwrap().starts_with("sha256:"));
    assert!(entry["to"].as_str().unwrap().starts_with("sha256:"));
}

// [FR-035]
#[test]
fn invalidなreqに紐づくエッジを指定したreconcileは拒否されロックは書き込まれない() {
    let dir = tempfile::tempdir().unwrap();
    common::write(
        dir.path(),
        ".miwa/config.yaml",
        common::default_config_yaml(),
    );
    common::write(
        dir.path(),
        "docs/spec.md",
        "---\ntype: Specification\nid: DOC-A\n---\n[リンク](../req/fr1.md)\n",
    );
    // FR-001 を id とする req ファイルが2つあり、duplicate id として invalid になる
    common::write(
        dir.path(),
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文A。\n",
    );
    common::write(
        dir.path(),
        "req/fr1-dup.md",
        "---\ntype: Requirement\nid: FR-001\n---\n本文B。\n",
    );

    common::run(dir.path(), &["reconcile", "--edge", "doc-req:DOC-A:FR-001"])
        .assert()
        .failure()
        .code(1);

    assert!(
        !dir.path().join(".miwa/lock.json").exists(),
        "lock must not be created on rejection"
    );
}

// [FR-035]
#[test]
fn brokenなtest_reqエッジを指定したreconcileは拒否されロックは書き込まれない() {
    let dir = tempfile::tempdir().unwrap();
    common::write(
        dir.path(),
        ".miwa/config.yaml",
        common::default_config_yaml(),
    );
    // マーカーが指す FR-999 は存在しない req id なので broken になる
    common::write(
        dir.path(),
        "tests/sample.rs",
        "// [FR-999]\n#[test]\nfn checks_fr_999() {\n    assert!(true);\n}\n",
    );

    common::run(
        dir.path(),
        &[
            "reconcile",
            "--edge",
            "test-req:tests/sample::checks_fr_999:FR-999",
        ],
    )
    .assert()
    .failure()
    .code(1);

    assert!(
        !dir.path().join(".miwa/lock.json").exists(),
        "lock must not be created on rejection"
    );
}

// [FR-036]
#[test]
fn allでreconcileした直後はgate付きcheckがfindingなしになる() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    common::run(dir.path(), &["reconcile", "--all"])
        .assert()
        .success();
    common::run(dir.path(), &["check", "--gate"])
        .assert()
        .code(0);
}

// [FR-033]
#[test]
fn edgeもallも指定しないreconcileはexit_code_1でロックを作らない() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    common::run(dir.path(), &["reconcile"])
        .assert()
        .failure()
        .code(1);

    assert!(
        !dir.path().join(".miwa/lock.json").exists(),
        "lock must not be created when neither --edge nor --all is given"
    );
}

// [FR-036]
#[test]
fn edgeとallを同時に指定するとreconcileは拒否される() {
    let dir = tempfile::tempdir().unwrap();
    common::write_minimal_fixture(dir.path());

    common::run(
        dir.path(),
        &["reconcile", "--edge", "doc-req:DOC-A:FR-001", "--all"],
    )
    .assert()
    .failure();

    assert!(
        !dir.path().join(".miwa/lock.json").exists(),
        "lock must not be created when --edge and --all conflict"
    );
}
