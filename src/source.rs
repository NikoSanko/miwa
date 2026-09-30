//! Abstraction over "a snapshot of the repository" (working tree or a git rev).

/// Lists and reads files of a repository snapshot. Paths are repo-relative,
/// `/`-separated, and sorted. Symlinks are not included. Paths under `.git`,
/// `target`, `.miwa`, `.worktrees`, and `node_modules` are excluded.
pub trait Source {
    fn list_files(&self) -> anyhow::Result<Vec<String>>;
    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>>;
}

pub struct FsSource {
    #[allow(dead_code)]
    root: std::path::PathBuf,
}

const EXCLUDED_DIRS: [&str; 5] = [".git", "target", ".miwa", ".worktrees", "node_modules"];

impl FsSource {
    pub fn new(root: impl Into<std::path::PathBuf>) -> FsSource {
        FsSource { root: root.into() }
    }
}

/// Rejects paths that are empty, absolute, or contain a `..` component.
fn reject_unsafe_path(path: &str) -> anyhow::Result<()> {
    if path.is_empty() {
        anyhow::bail!("empty path is not allowed");
    }
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        anyhow::bail!("absolute path is not allowed: {path}");
    }
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        anyhow::bail!("path must not contain '..': {path}");
    }
    Ok(())
}

impl Source for FsSource {
    fn list_files(&self) -> anyhow::Result<Vec<String>> {
        let root = &self.root;
        let mut files = Vec::new();
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                if entry.file_type().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if EXCLUDED_DIRS.contains(&name) {
                            return false;
                        }
                    }
                }
                true
            });
        for entry in walker {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry.path().strip_prefix(root)?;
            let rel_str = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.push(rel_str);
        }
        files.sort();
        Ok(files)
    }

    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        reject_unsafe_path(path)?;
        Ok(std::fs::read(self.root.join(path))?)
    }
}

pub struct GitTreeSource {
    repo: std::path::PathBuf,
    rev: String,
    /// The full, unfiltered `path -> blob oid` map for `rev` (no directory
    /// exclusions applied). `list_files` filters this; `read` looks paths up
    /// in it directly, so files under excluded directories (e.g. `.miwa/`)
    /// can still be read even though they are not listed.
    files: std::collections::BTreeMap<String, String>,
}

fn run_git(repo: &std::path::Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()?;
    Ok(output)
}

impl GitTreeSource {
    /// Validates `rev` against `repo` and eagerly lists every blob in its
    /// tree (unfiltered).
    pub fn new(repo: impl Into<std::path::PathBuf>, rev: &str) -> anyhow::Result<GitTreeSource> {
        let repo = repo.into();
        let output = run_git(
            &repo,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )?;
        if !output.status.success() {
            anyhow::bail!(
                "git rev-parse --verify failed for {rev}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let ls_output = run_git(&repo, &["ls-tree", "-r", "-z", "--full-tree", rev])?;
        if !ls_output.status.success() {
            anyhow::bail!(
                "git ls-tree failed: {}",
                String::from_utf8_lossy(&ls_output.stderr)
            );
        }
        let stdout = String::from_utf8_lossy(&ls_output.stdout);
        let mut files = std::collections::BTreeMap::new();
        for entry in stdout.split('\0') {
            if entry.is_empty() {
                continue;
            }
            let Some((meta, path)) = entry.split_once('\t') else {
                continue;
            };
            let mut parts = meta.split_whitespace();
            let mode = parts.next().unwrap_or("");
            let obj_type = parts.next().unwrap_or("");
            let oid = parts.next().unwrap_or("");
            if obj_type != "blob" {
                continue;
            }
            // Mode 120000 is a symlink; `Source` never follows symlinks
            // (matches `FsSource`'s `follow_links(false)`), so exclude them.
            if mode == "120000" {
                continue;
            }
            files.insert(path.to_string(), oid.to_string());
        }

        Ok(GitTreeSource {
            repo,
            rev: rev.to_string(),
            files,
        })
    }
}

impl Source for GitTreeSource {
    fn list_files(&self) -> anyhow::Result<Vec<String>> {
        let mut files: Vec<String> = self
            .files
            .keys()
            .filter(|path| {
                !path
                    .split('/')
                    .any(|segment| EXCLUDED_DIRS.contains(&segment))
            })
            .cloned()
            .collect();
        files.sort();
        Ok(files)
    }

    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let Some(oid) = self.files.get(path) else {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{path} does not exist at {}", self.rev),
            )));
        };
        let output = run_git(&self.repo, &["cat-file", "blob", oid])?;
        if !output.status.success() {
            anyhow::bail!(
                "git cat-file blob failed for {path}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(output.stdout)
    }
}

/// Whether `err` is a `Source::read` failure caused by the path not existing
/// (as opposed to, say, a `git` invocation failure). Callers use this to
/// distinguish "file absent" from a real execution error.
pub fn is_not_found(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>()
        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}

/// Returns the first line of `git merge-base <base> HEAD` run in `repo`.
pub fn merge_base(repo: &std::path::Path, base: &str) -> anyhow::Result<String> {
    let output = run_git(repo, &["merge-base", base, "HEAD"])?;
    if !output.status.success() {
        anyhow::bail!(
            "git merge-base failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim().to_string();
    if first_line.is_empty() {
        anyhow::bail!("git merge-base returned no output");
    }
    Ok(first_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // [FR-018]
    #[test]
    fn 除外ディレクトリ配下のファイルは列挙されない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("req")).unwrap();
        fs::write(root.join("req/fr-001.md"), "ok").unwrap();
        for excluded in [".git", "target", ".miwa", ".worktrees", "node_modules"] {
            let sub = root.join(excluded).join("nested");
            fs::create_dir_all(&sub).unwrap();
            fs::write(sub.join("file.md"), "should not be listed").unwrap();
        }

        let source = FsSource::new(root);
        let files = source.list_files().unwrap();
        assert!(files.contains(&"req/fr-001.md".to_string()));
        for excluded in [".git", "target", ".miwa", ".worktrees", "node_modules"] {
            assert!(
                !files.iter().any(|f| f.starts_with(&format!("{excluded}/"))),
                "excluded dir {excluded} leaked into: {files:?}"
            );
        }
    }

    // [FR-018]
    #[cfg(unix)]
    #[test]
    fn シンボリックリンクの先は辿らない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("req")).unwrap();
        fs::write(root.join("req/fr-001.md"), "ok").unwrap();

        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "should not be visible").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("req/linked")).unwrap();

        let source = FsSource::new(root);
        let files = source.list_files().unwrap();
        assert!(!files.iter().any(|f| f.contains("secret.md")));
    }

    // [FR-018]
    #[test]
    fn fssourceのreadは絶対パスやドットドットや空パスを拒否する() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.txt"), "ok").unwrap();

        let source = FsSource::new(root);
        assert!(source.read("").is_err());
        assert!(source.read("../a.txt").is_err());
        assert!(source.read("sub/../../a.txt").is_err());
        #[cfg(unix)]
        assert!(source.read("/etc/passwd").is_err());
        assert!(source.read("a.txt").is_ok());
    }

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
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

    fn init_repo(root: &std::path::Path) {
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.name", "Test User"]);
        git(root, &["config", "user.email", "test@example.com"]);
    }

    // [FR-021]
    #[test]
    fn gittreesourceは除外ディレクトリ配下もreadでは読める() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::create_dir_all(root.join(".miwa")).unwrap();
        fs::write(root.join(".miwa/config.yaml"), "docs:\n  - docs\n").unwrap();
        fs::create_dir_all(root.join("req")).unwrap();
        fs::write(root.join("req/fr1.md"), "content").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);

        let source = GitTreeSource::new(root, "HEAD").unwrap();
        let files = source.list_files().unwrap();
        assert!(
            !files.iter().any(|f| f.starts_with(".miwa/")),
            ".miwa should be excluded from list_files: {files:?}"
        );
        let content = source.read(".miwa/config.yaml").unwrap();
        assert_eq!(String::from_utf8(content).unwrap(), "docs:\n  - docs\n");
    }

    // [FR-021]
    #[test]
    fn gittreesourceのreadは存在しないパスでnotfoundエラーを返す() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::write(root.join("a.txt"), "hi").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);

        let source = GitTreeSource::new(root, "HEAD").unwrap();
        let err = source.read("does/not/exist.md").unwrap_err();
        let io_err = err
            .downcast_ref::<std::io::Error>()
            .expect("error should be a std::io::Error");
        assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
    }

    // [FR-018]
    #[test]
    fn gittreesourceは除外ディレクトリ配下をlist_filesに含めない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::create_dir_all(root.join("target/nested")).unwrap();
        fs::write(root.join("target/nested/file.txt"), "x").unwrap();
        fs::write(root.join("keep.txt"), "y").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);

        let source = GitTreeSource::new(root, "HEAD").unwrap();
        let files = source.list_files().unwrap();
        assert!(files.contains(&"keep.txt".to_string()));
        assert!(
            !files.iter().any(|f| f.starts_with("target/")),
            "target/ should be excluded: {files:?}"
        );
    }

    // [FR-021]
    #[test]
    fn gittreesourceのreadはコミット時点の内容を返す() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::write(root.join("a.txt"), "v1").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "v1"]);
        let rev = git(root, &["rev-parse", "HEAD"]);

        fs::write(root.join("a.txt"), "v2").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "v2"]);

        let source = GitTreeSource::new(root, &rev).unwrap();
        let content = source.read("a.txt").unwrap();
        assert_eq!(String::from_utf8(content).unwrap(), "v1");
    }

    // [FR-018]
    #[cfg(unix)]
    #[test]
    fn gittreesourceはシンボリックリンクをlist_filesに含めない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::write(root.join("real.txt"), "content").unwrap();
        std::os::unix::fs::symlink("real.txt", root.join("linked.txt")).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "init"]);

        let source = GitTreeSource::new(root, "HEAD").unwrap();
        let files = source.list_files().unwrap();
        assert!(files.contains(&"real.txt".to_string()));
        assert!(
            !files.contains(&"linked.txt".to_string()),
            "symlinks should be excluded: {files:?}"
        );
    }
}
