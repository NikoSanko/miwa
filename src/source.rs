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

impl FsSource {
    pub fn new(root: impl Into<std::path::PathBuf>) -> FsSource {
        let _ = root;
        todo!()
    }
}

impl Source for FsSource {
    fn list_files(&self) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let _ = path;
        todo!()
    }
}

pub struct GitTreeSource {
    #[allow(dead_code)]
    repo: std::path::PathBuf,
    #[allow(dead_code)]
    rev: String,
}

impl GitTreeSource {
    /// Validates `rev` against `repo`.
    pub fn new(repo: impl Into<std::path::PathBuf>, rev: &str) -> anyhow::Result<GitTreeSource> {
        let _ = (repo, rev);
        todo!()
    }
}

impl Source for GitTreeSource {
    fn list_files(&self) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let _ = path;
        todo!()
    }
}

/// Returns the first line of `git merge-base <base> HEAD` run in `repo`.
pub fn merge_base(repo: &std::path::Path, base: &str) -> anyhow::Result<String> {
    let _ = (repo, base);
    todo!()
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
}
