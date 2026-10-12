//! `miwa site --diff`: a single-page HTML view of document changes.

use crate::config::Config;

/// Default output path when `--diff` is given without `--out`.
pub const DEFAULT_OUT: &str = "_site-diff.html";

/// Options for `miwa site --diff`.
#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    pub base_ref: Option<String>,
    pub out: Option<String>,
    pub link_base: Option<String>,
    pub head_label: Option<String>,
}

/// Writes the diff HTML (FR-108..120). Ok(()) also when nothing changed (no file written).
pub fn run(
    _repo_root: &std::path::Path,
    _config: &Config,
    _opts: &DiffOptions,
) -> anyhow::Result<()> {
    todo!()
}
