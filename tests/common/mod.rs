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

/// The default `.miwa/config.yaml` used by most fixtures: docs/req/tests dirs,
/// as plain top-level directories under the fixture root.
pub fn default_config_yaml() -> &'static str {
    "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - tests\ndiff_base: main\njudge:\n  threshold: 0.9\n  llm_model: claude-sonnet-5\n"
}

/// Runs `miwa` with `args` in `dir` and returns the `assert_cmd::Command` (unasserted).
pub fn run(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("miwa").unwrap();
    cmd.current_dir(dir).args(args);
    cmd
}

/// A minimal fixture: one doc linking to one req, and one rust test verifying it.
pub fn write_minimal_fixture(dir: &Path) {
    write(dir, ".miwa/config.yaml", default_config_yaml());
    write(
        dir,
        "docs/spec.md",
        "---\ntype: Specification\nid: DOC-A\n---\n# Spec\n\nSee [req](../req/fr1.md).\n",
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
