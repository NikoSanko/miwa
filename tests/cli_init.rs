mod common;

// [FR-019]
#[test]
fn initは既定値でconfigを生成する() {
    let dir = tempfile::tempdir().unwrap();
    common::run(dir.path(), &["init"]).assert().success();

    let config_path = dir.path().join(".miwa/config.yaml");
    assert!(config_path.exists(), "config.yaml should be created");
    let content = std::fs::read_to_string(&config_path).unwrap();

    // 既定値: docs=["docs"], requirements=["req"], tests=["."], diff_base="main", threshold=0.9
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content).unwrap();
    assert_eq!(value["docs"][0].as_str(), Some("docs"));
    assert_eq!(value["requirements"][0].as_str(), Some("req"));
    assert_eq!(value["tests"][0].as_str(), Some("."));
    assert_eq!(value["diff_base"].as_str(), Some("main"));
    assert_eq!(value["judge"]["threshold"].as_f64(), Some(0.9));
}

// [FR-019]
#[test]
fn 既にconfigが存在する場合は上書きせず失敗する() {
    let dir = tempfile::tempdir().unwrap();
    let existing = "docs:\n  - custom-docs\nrequirements:\n  - custom-req\ntests:\n  - custom-tests\ndiff_base: develop\njudge:\n  threshold: 0.5\n  llm_model: custom-model\n";
    common::write(dir.path(), ".miwa/config.yaml", existing);

    common::run(dir.path(), &["init"])
        .assert()
        .failure()
        .code(1);

    let content = std::fs::read_to_string(dir.path().join(".miwa/config.yaml")).unwrap();
    assert_eq!(content, existing, "existing config must not be overwritten");
}
