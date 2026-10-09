//! Fixture helpers for CLI integration tests.
#![allow(dead_code)]

use assert_cmd::Command;
use std::path::Path;
use std::process::Command as StdCommand;

/// Writes `content` to `<dir>/<rel_path>`, creating parent directories as needed.
pub fn write(dir: &Path, rel_path: &str, content: &str) {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&full, content).unwrap();
}

/// `git init -b main` in `dir`, with a fixed local user identity.
pub fn git_init(dir: &Path) {
    run_git(dir, &["init", "-b", "main"]);
    run_git(dir, &["config", "user.name", "Test User"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
}

/// `git add -A && git commit -m <msg>` in `dir`.
pub fn git_commit(dir: &Path, msg: &str) {
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-m", msg]);
}

/// `git checkout -b <name>` in `dir`.
pub fn git_checkout_new_branch(dir: &Path, name: &str) {
    run_git(dir, &["checkout", "-b", name]);
}

/// Runs an arbitrary git subcommand in `dir` and returns stdout (trimmed).
pub fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A typed document source with `title` / `description` front matter.
pub fn typed(title: &str, description: Option<&str>) -> String {
    match description {
        Some(d) => format!("---\ntype: BusinessNeed\ntitle: {title}\ndescription: {d}\n---\n"),
        None => format!("---\ntype: BusinessNeed\ntitle: {title}\n---\n"),
    }
}

/// A `.miwa/config.yaml` without `index` (docs/req/tests dirs as plain
/// top-level directories under the fixture root). Differs from what `init`
/// writes, which includes `index`.
pub fn config_yaml_without_index() -> &'static str {
    "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n"
}

/// Runs `miwa` with `args` in `dir` and returns the `assert_cmd::Command` (unasserted).
pub fn run(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("miwa").unwrap();
    cmd.current_dir(dir).args(args);
    cmd
}

/// A minimal fixture: one doc linking to one req, and one rust test verifying it.
pub fn write_minimal_fixture(dir: &Path) {
    write(dir, ".miwa/config.yaml", config_yaml_without_index());
    write(
        dir,
        "docs/spec.md",
        "---\ntype: BusinessNeed\nid: DOC-A\n---\n# Spec\n\nSee [req](../req/fr1.md).\n",
    );
    write(
        dir,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n最初の要件本文。\n",
    );
    write(
        dir,
        "tests/sample.rs",
        "// [FR-001]\n#[test]\nfn checks_fr_001() {\n    assert!(true);\n}\n",
    );
}

/// A request recorded by `MockServer`.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    /// Header names are lower-cased.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl RecordedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A tiny HTTP server on `127.0.0.1:0` that records requests and replies with
/// the given `(status, body)` responses in order (the last one repeats).
pub struct MockServer {
    pub url: String,
    requests: std::sync::Arc<std::sync::Mutex<Vec<RecordedRequest>>>,
}

impl MockServer {
    pub fn start(responses: Vec<(u16, String)>) -> MockServer {
        use std::io::{BufRead, BufReader, Read, Write};
        assert!(!responses.is_empty());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = requests.clone();
        std::thread::spawn(move || {
            let mut index = 0usize;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    continue;
                }
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut headers = Vec::new();
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let line = line.trim_end().to_string();
                    if line.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        let k = k.trim().to_ascii_lowercase();
                        let v = v.trim().to_string();
                        if k == "content-length" {
                            content_length = v.parse().unwrap_or(0);
                        }
                        headers.push((k, v));
                    }
                }
                let mut body = vec![0u8; content_length];
                let _ = reader.read_exact(&mut body);
                recorded.lock().unwrap().push(RecordedRequest {
                    method,
                    path,
                    headers,
                    body: String::from_utf8_lossy(&body).into_owned(),
                });
                let (status, resp_body) = &responses[index.min(responses.len() - 1)];
                index += 1;
                let reply = format!(
                    "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                    resp_body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            }
        });
        MockServer {
            url: format!("http://{addr}/v1/systemone"),
            requests,
        }
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

/// A Jev response body carrying `score`.
pub fn jev_response(score: f64) -> String {
    format!(
        r#"{{"model":"jev-1.13.0","answers":{{"consistent":{{"type":"noul","noul":{score}}}}},"usage":{{"input_tokens":1,"output_tokens":1}}}}"#
    )
}

pub const DUMMY_API_KEY: &str = "sk-dummy-typesafe";

/// Fixture for `judge`: the minimal fixture (edges `doc-req:DOC-A:FR-001` and
/// `test-req:tests/sample::checks_fr_001:FR-001`) with the default config.
pub fn write_judge_fixture(dir: &Path) {
    write_minimal_fixture(dir);
}

/// Makes every edge of the minimal fixture drift: reconciles all edges, then edits the req.
pub fn make_edges_drift(dir: &Path) {
    run(dir, &["reconcile", "--all"]).assert().success();
    write(
        dir,
        "req/fr1.md",
        "---\ntype: Requirement\nid: FR-001\n---\n変更後の要件本文。\n",
    );
}

/// `miwa judge <args>` in `dir` with the API key and the mock endpoint set.
pub fn run_judge(dir: &Path, server: &MockServer, args: &[&str]) -> Command {
    let mut full = vec!["judge"];
    full.extend_from_slice(args);
    let mut cmd = run(dir, &full);
    cmd.env("TYPESAFE_API_KEY", DUMMY_API_KEY)
        .env("MIWA_JEV_ENDPOINT", &server.url);
    cmd
}
