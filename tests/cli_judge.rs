mod common;

use common::{
    jev_response, make_edges_drift, recording_agent_script, run_judge, write_judge_fixture,
    MockServer, DUMMY_API_KEY,
};

const DOC_EDGE: &str = "doc-req:DOC-A:FR-001";
const TEST_EDGE: &str = "test-req:tests/sample::checks_fr_001:FR-001";

struct Env {
    repo: tempfile::TempDir,
    agent: tempfile::TempDir,
}

impl Env {
    /// Both edges are `new` (no lock yet).
    fn new() -> Env {
        let repo = tempfile::tempdir().unwrap();
        let agent = tempfile::tempdir().unwrap();
        write_judge_fixture(repo.path(), &recording_agent_script(agent.path()));
        Env { repo, agent }
    }

    /// Both edges are `drift` (locked, then the req edited).
    fn drifted() -> Env {
        let env = Env::new();
        make_edges_drift(env.repo.path());
        env
    }

    fn prompt_path(&self) -> std::path::PathBuf {
        self.agent.path().join("prompt.txt")
    }

    fn prompt(&self) -> String {
        std::fs::read_to_string(self.prompt_path()).expect("agent should have recorded a prompt")
    }

    fn lock_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.repo.path().join(".miwa/lock.json")).ok()
    }
}

fn ok(score: f64) -> (u16, String) {
    (200, jev_response(score))
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

// [FR-038]
#[test]
fn judgeはdriftとnewのエッジを一次判定にかける() {
    let env = Env::drifted();
    let server = MockServer::start(vec![ok(0.95)]);
    run_judge(env.repo.path(), &server, &[]).assert().success();
    let bodies: Vec<serde_json::Value> = server
        .requests()
        .iter()
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0]["state"].get("doc").is_some(), "{}", bodies[0]);
    assert!(bodies[1]["state"].get("test").is_some(), "{}", bodies[1]);
}

// [FR-038]
#[test]
fn judgeのedge指定は対象外のidならapiもエージェントも呼ばずexit1になる() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    // 対象は new の 2 本。存在しない ID は対象外
    let output = run_judge(
        env.repo.path(),
        &server,
        &["--edge", DOC_EDGE, "--edge", "doc-req:NOPE:FR-404"],
    )
    .output()
    .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(server.requests().is_empty());
    assert!(!env.prompt_path().exists());
}

// [FR-038]
#[test]
fn judgeのedge指定は指定したエッジだけを判定する() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    let output = run_judge(
        env.repo.path(),
        &server,
        &["--edge", TEST_EDGE, "--edge", TEST_EDGE],
    )
    .output()
    .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(server.requests().len(), 1);
    let stdout = stdout_of(&output);
    assert!(stdout.contains(TEST_EDGE), "{stdout}");
    assert!(!stdout.contains(DOC_EDGE), "{stdout}");
}

// [FR-038]
#[test]
fn judgeはformatオプションを持たない() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    let output = run_judge(env.repo.path(), &server, &["--format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_ne!(output.status.code(), Some(0));
    assert!(server.requests().is_empty());
}

// [FR-039]
#[test]
fn apiキーが未設定なら何も送信せずキー名を表示してexit1になる() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[])
        .env_remove("TYPESAFE_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("TYPESAFE_API_KEY"));
    assert!(server.requests().is_empty());
    assert!(!env.prompt_path().exists());
}

// [FR-039]
#[test]
fn apiキーが空文字列なら何も送信せずexit1になる() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[])
        .env("TYPESAFE_API_KEY", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("TYPESAFE_API_KEY"));
    assert!(server.requests().is_empty());
    assert!(!env.prompt_path().exists());
}

// [FR-039]
#[test]
fn apiキーの確認は対象エッジが0件でも行う() {
    let env = Env::new();
    common::run(env.repo.path(), &["reconcile", "--all"])
        .assert()
        .success();
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[])
        .env_remove("TYPESAFE_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("TYPESAFE_API_KEY"));
}

// [FR-040]
#[test]
fn 実行開始時に外部送信する旨をstderrに1行で表示する() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stderr = stderr_of(&output);
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("api.typesafe.ai"))
        .collect();
    assert_eq!(lines.len(), 1, "stderr: {stderr}");
}

// [FR-040]
#[test]
fn apiキー未設定で終了する場合は外部送信の表示を行わない() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    let output = run_judge(env.repo.path(), &server, &[])
        .env_remove("TYPESAFE_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!stderr_of(&output).contains("api.typesafe.ai"));
}

// [FR-041]
#[test]
fn jevにはbearer認証のpostでmodelを指定して送る() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    run_judge(env.repo.path(), &server, &[]).assert().success();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let r = &requests[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path, "/v1/systemone");
    assert_eq!(
        r.header("authorization"),
        Some(format!("Bearer {DUMMY_API_KEY}").as_str())
    );
    assert!(r
        .header("content-type")
        .unwrap()
        .starts_with("application/json"));
    let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["model"], "jev-latest");
    assert_eq!(body["questions"]["consistent"]["type"], "noul");
    assert!(body["state"]["req"]
        .as_str()
        .unwrap()
        .contains("最初の要件本文"));
}

// [FR-042]
#[test]
fn jevの応答からスコアが取れなければexit1になりエージェントは起動しない() {
    let env = Env::new();
    let server = MockServer::start(vec![(200, r#"{"answers":{}}"#.to_string())]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!env.prompt_path().exists());
}

// [FR-043]
#[test]
fn jevが503を返したらexit1になりapiキーをstderrに出さずエージェントも起動しない() {
    let env = Env::new();
    let server = MockServer::start(vec![(503, "unavailable".to_string())]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_of(&output);
    assert!(!stderr.contains(DUMMY_API_KEY), "{stderr}");
    assert!(!stderr.contains("Bearer"), "{stderr}");
    assert!(!env.prompt_path().exists());
}

// [FR-044]
#[test]
fn スコアが閾値以上のエッジだけconsistentとして表示する() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.9), ok(0.89)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains(&format!("consistent {DOC_EDGE} score=0.90")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("review {TEST_EDGE} score=0.89")),
        "{stdout}"
    );
}

// [FR-045]
#[test]
fn reviewが複数あってもエージェントは1回だけリポジトリルートで起動される() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let runs = std::fs::read_to_string(env.agent.path().join("runs.txt")).unwrap();
    assert_eq!(runs.lines().count(), 1, "runs: {runs}");
    let cwd = std::fs::read_to_string(env.agent.path().join("cwd.txt")).unwrap();
    assert_eq!(
        std::fs::canonicalize(cwd.trim()).unwrap(),
        std::fs::canonicalize(env.repo.path()).unwrap()
    );
}

// [FR-045]
#[test]
fn エージェントの出力は加工せずstdoutに流れる() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert!(stdout_of(&output).contains("AGENT-OUTPUT"));
}

// [FR-045]
#[test]
fn reviewが0件ならエージェントは起動しない() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(!env.agent.path().join("runs.txt").exists());
    assert!(!stdout_of(&output).contains("AGENT-OUTPUT"));
}

// [FR-045]
#[test]
fn エージェントが起動できなければexit1になる() {
    let env = Env::new();
    let command = serde_json::json!(["miwa-no-such-agent-xyz"]);
    common::write(
        env.repo.path(),
        ".miwa/config.yaml",
        &format!(
            "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n  llm_command: {command}\n"
        ),
    );
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!stderr_of(&output).contains(DUMMY_API_KEY));
}

// [FR-046]
#[test]
fn プロンプトにはreviewのエッジの詳細が入りconsistentのエッジは入らない() {
    let env = Env::new();
    // doc-req が consistent、test-req が review
    let server = MockServer::start(vec![ok(0.95), ok(0.3)]);
    run_judge(env.repo.path(), &server, &[]).assert().success();
    let prompt = env.prompt();
    assert!(prompt.contains(TEST_EDGE), "{prompt}");
    assert!(prompt.contains("tests/sample.rs"), "{prompt}");
    assert!(prompt.contains("req/fr1.md"), "{prompt}");
    assert!(prompt.contains("checks_fr_001"), "{prompt}");
    assert!(prompt.contains("最初の要件本文"), "{prompt}");
    assert!(prompt.contains("0.30"), "{prompt}");
    assert!(!prompt.contains(DOC_EDGE), "{prompt}");
}

// [FR-046]
#[test]
fn プロンプトのエッジはid昇順に並ぶ() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    run_judge(env.repo.path(), &server, &[]).assert().success();
    let prompt = env.prompt();
    let doc = prompt.find(DOC_EDGE).expect(&prompt);
    let test = prompt.find(TEST_EDGE).expect(&prompt);
    assert!(doc < test, "{prompt}");
}

// [FR-047]
#[test]
fn auto_reconcile指定ありのプロンプトはreconcileの実行を指示する() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    run_judge(env.repo.path(), &server, &["--auto-reconcile"])
        .assert()
        .success();
    let prompt = env.prompt();
    assert!(prompt.contains("miwa reconcile --edge"), "{prompt}");
    assert!(prompt.contains("--reason"), "{prompt}");
}

// [FR-047]
#[test]
fn auto_reconcile指定なしのプロンプトはreconcileを実行しないと明記する() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    run_judge(env.repo.path(), &server, &[]).assert().success();
    let prompt = env.prompt();
    assert!(prompt.contains("reconcile は実行しない"), "{prompt}");
    assert!(!prompt.contains("miwa reconcile --edge"), "{prompt}");
}

// [FR-048]
#[test]
fn 本文が上限を超えるエッジはskippedになりjevにもエージェントにも渡らない() {
    let env = Env::new();
    common::write(
        env.repo.path(),
        "req/fr1.md",
        &format!(
            "---\ntype: Requirement\nid: FR-001\n---\n{}\n",
            "あ".repeat(70_000)
        ),
    );
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(server.requests().is_empty());
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains(&format!("skipped {DOC_EDGE} score=-")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("skipped {TEST_EDGE} score=-")),
        "{stdout}"
    );
    assert!(!env.agent.path().join("runs.txt").exists());
}

// [FR-049]
#[test]
fn 一覧はエッジid昇順の1行1エッジで表示されエージェントの出力より前に出る() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.94), ok(0.31)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout_of(&output);
    let first = format!("consistent {DOC_EDGE} score=0.94");
    let second = format!("review {TEST_EDGE} score=0.31");
    let p1 = stdout.find(&first).expect(&stdout);
    let p2 = stdout.find(&second).expect(&stdout);
    let agent = stdout.find("AGENT-OUTPUT").expect(&stdout);
    assert!(p1 < p2 && p2 < agent, "{stdout}");
}

// [FR-050]
#[test]
fn 対象が0件なら対象なしと表示してexit0になりapiもエージェントも呼ばない() {
    let env = Env::new();
    common::run(env.repo.path(), &["reconcile", "--all"])
        .assert()
        .success();
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_of(&output).contains("対象なし"));
    assert!(server.requests().is_empty());
    assert!(!env.prompt_path().exists());
}

// [FR-050]
#[test]
fn reviewやskippedがあっても判定が完了すればexit0になる() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.3)]);
    run_judge(env.repo.path(), &server, &[])
        .assert()
        .success()
        .code(0);
}

// [FR-050]
#[test]
fn エージェントが非0で終了したらexit1になる() {
    let env = Env::new();
    write_judge_fixture(
        env.repo.path(),
        &format!("cat > {}/prompt.txt; exit 2", env.agent.path().display()),
    );
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
}

// [FR-050]
#[test]
fn 設定エラーならexit1になり何も送信しない() {
    let env = Env::new();
    common::write(
        env.repo.path(),
        ".miwa/config.yaml",
        "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n  llm_command: []\n",
    );
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(env.repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(server.requests().is_empty());
}

// [FR-051]
#[test]
fn auto_reconcileはconsistentのエッジだけをjevの合意としてロックに書く() {
    let env = Env::new();
    let server = MockServer::start(vec![ok(0.95), ok(0.3)]);
    run_judge(env.repo.path(), &server, &["--auto-reconcile"])
        .assert()
        .success();
    let lock: serde_json::Value = serde_json::from_slice(&env.lock_bytes().unwrap()).unwrap();
    let edges = lock["edges"].as_object().unwrap();
    assert_eq!(edges.len(), 1, "{lock}");
    let e = &lock["edges"][DOC_EDGE];
    assert_eq!(e["by"], "jev");
    assert_eq!(e["score"], 0.95);
    assert!(e.get("reason").is_none(), "{e}");
    assert!(e["from"].as_str().unwrap().starts_with("sha256:"));
    assert!(e["to"].as_str().unwrap().starts_with("sha256:"));
    assert!(e["at"].as_str().is_some());
}

// [FR-051]
#[test]
fn auto_reconcileなしならロックは変更されない() {
    let env = Env::drifted();
    let before = env.lock_bytes().unwrap();
    let server = MockServer::start(vec![ok(0.95)]);
    run_judge(env.repo.path(), &server, &[]).assert().success();
    assert_eq!(env.lock_bytes().unwrap(), before);
}

// [FR-051]
#[test]
fn 一次判定の途中でエラーになったらロックは変更されない() {
    let env = Env::drifted();
    let before = env.lock_bytes().unwrap();
    let server = MockServer::start(vec![ok(0.95), (503, "unavailable".to_string())]);
    let output = run_judge(env.repo.path(), &server, &["--auto-reconcile"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(env.lock_bytes().unwrap(), before);
}

// [FR-051]
#[test]
fn auto_reconcileはエージェントを起動する前にロックを書く() {
    let env = Env::new();
    let lock_path = env.repo.path().join(".miwa/lock.json");
    // エージェントが起動した時点でのロックを記録する
    write_judge_fixture(
        env.repo.path(),
        &format!(
            "cp {} {}/lock-at-agent.json; cat > {}/prompt.txt",
            lock_path.display(),
            env.agent.path().display(),
            env.agent.path().display()
        ),
    );
    let server = MockServer::start(vec![ok(0.95), ok(0.3)]);
    run_judge(env.repo.path(), &server, &["--auto-reconcile"])
        .assert()
        .success();
    let snapshot = std::fs::read_to_string(env.agent.path().join("lock-at-agent.json")).unwrap();
    assert!(snapshot.contains(DOC_EDGE), "{snapshot}");
    assert!(snapshot.contains("\"jev\""), "{snapshot}");
}

// [FR-045]
#[test]
fn エージェントの子プロセスにapiキーとendpoint上書きは継承されない() {
    let repo = tempfile::tempdir().unwrap();
    let agent = tempfile::tempdir().unwrap();
    let script = format!("cat > /dev/null; env > {}/env.txt", agent.path().display());
    write_judge_fixture(repo.path(), &script);
    let server = MockServer::start(vec![ok(0.3)]);
    let output = run_judge(repo.path(), &server, &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let env = std::fs::read_to_string(agent.path().join("env.txt")).unwrap();
    assert!(!env.contains("TYPESAFE_API_KEY"), "{env}");
    assert!(!env.contains("MIWA_JEV_ENDPOINT"), "{env}");
    assert!(!env.contains(DUMMY_API_KEY), "{env}");
}
