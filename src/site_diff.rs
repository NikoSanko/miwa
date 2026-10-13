//! `miwa site --diff`: a single-page HTML view of document changes.
//!
//! The documents of the merge-base and of the working tree are loaded with
//! the same code as `miwa site`, paired (FR-112), and every changed one is
//! shown as a frame of aligned blocks (FR-108 .. FR-120).

mod pair;
mod render;
mod style;

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use sha2::Digest;

use crate::config::{Config, CONFIG_PATH};
use crate::site::{esc, load_side, unpublished_candidates, Candidate, SideModel};
use crate::source::{is_not_found, merge_base, run_git, FsSource, GitTreeSource, Source};
use pair::{Item, Status};
use render::{icon, render_frame, Ctx};

/// Default output path when `--diff` is given without `--out`.
pub const DEFAULT_OUT: &str = "_site-diff.html";

const CSP: &str = "default-src 'none'; img-src 'self' data: https:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'";

/// Options for `miwa site --diff`.
#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    pub base_ref: Option<String>,
    pub out: Option<String>,
    pub link_base: Option<String>,
    pub head_label: Option<String>,
}

/// Hashes and embeds the images of one side. Each file is read once for its
/// hash and once for its data URI. A read failure is remembered (the first
/// one) and reported by [`Images::finish`], so that nothing is written.
struct Images<'a> {
    source: &'a dyn Source,
    /// Where the side's files come from, for error messages.
    origin: String,
    hashes: RefCell<HashMap<String, Option<String>>>,
    uris: RefCell<HashMap<String, Option<String>>>,
    failure: RefCell<Option<anyhow::Error>>,
}

impl Images<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self.source.read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                let mut failure = self.failure.borrow_mut();
                if failure.is_none() {
                    *failure =
                        Some(e.context(format!("failed to read image {path} at {}", self.origin)));
                }
                None
            }
        }
    }

    /// The SHA-256 of the image file, `None` when it cannot be read.
    fn hash(&self, path: &str) -> Option<String> {
        if let Some(h) = self.hashes.borrow().get(path) {
            return h.clone();
        }
        let hash = self
            .read(path)
            .map(|bytes| hex::encode(sha2::Sha256::digest(&bytes)));
        self.hashes
            .borrow_mut()
            .insert(path.to_string(), hash.clone());
        hash
    }

    /// The image as a `data:` URI (built once per file), `None` when it
    /// cannot be read.
    fn data_uri(&self, path: &str) -> Option<String> {
        if let Some(u) = self.uris.borrow().get(path) {
            return u.clone();
        }
        let mime = match path.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => return None,
        };
        let uri = self
            .read(path)
            .map(|bytes| format!("data:{mime};base64,{}", base64(&bytes)));
        self.uris.borrow_mut().insert(path.to_string(), uri.clone());
        uri
    }

    /// Fails with the first image read error, if any.
    fn finish(&self) -> anyhow::Result<()> {
        match self.failure.borrow_mut().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// One side of the comparison.
struct Side<'a> {
    /// `b` (base) or `h` (head).
    tag: char,
    model: SideModel,
    /// Typed files that the side does not publish.
    candidates: Vec<Candidate>,
    images: Images<'a>,
}

impl<'a> Side<'a> {
    fn load(
        tag: char,
        origin: &str,
        source: &'a dyn Source,
        config: &Config,
    ) -> anyhow::Result<Side<'a>> {
        let model = load_side(source, config)?;
        let candidates = unpublished_candidates(source, config, &model)?;
        Ok(Side::new(tag, origin, source, model, candidates))
    }

    fn new(
        tag: char,
        origin: &str,
        source: &'a dyn Source,
        model: SideModel,
        candidates: Vec<Candidate>,
    ) -> Side<'a> {
        Side {
            tag,
            model,
            candidates,
            images: Images {
                source,
                origin: origin.to_string(),
                hashes: RefCell::new(HashMap::new()),
                uris: RefCell::new(HashMap::new()),
                failure: RefCell::new(None),
            },
        }
    }

    /// The published images the page references, with their content hashes.
    fn image_hashes(&self, path: &str, body: &str) -> Vec<(String, Option<String>)> {
        self.model
            .referenced_images(path, body)
            .into_iter()
            .map(|image| {
                let hash = self.images.hash(&image);
                (image, hash)
            })
            .collect()
    }
}

/// `--link-base`: `http:` / `https:` only, with a trailing `/`.
fn normalize_link_base(raw: &str) -> anyhow::Result<String> {
    let lower = raw.to_ascii_lowercase();
    if !(lower.starts_with("http:") || lower.starts_with("https:")) {
        anyhow::bail!("--link-base must start with http: or https:");
    }
    Ok(if raw.ends_with('/') {
        raw.to_string()
    } else {
        format!("{raw}/")
    })
}

/// Nothing may exist at the output path (not even a broken symbolic link).
fn check_out_path(out: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(out) {
        Ok(_) => anyhow::bail!("output path {} already exists", out.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::Error::new(e)
            .context(format!("failed to inspect output path {}", out.display()))),
    }
}

/// The first seven characters of a commit id.
fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// The current branch name, `HEAD` when detached.
fn current_branch(repo_root: &Path) -> String {
    run_git(repo_root, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "HEAD".to_string())
}

fn head_sha(repo_root: &Path) -> anyhow::Result<String> {
    let output = run_git(repo_root, &["rev-parse", "HEAD"])?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The configuration at the merge-base; `None` when there is none.
fn base_config(source: &dyn Source, merge_base_rev: &str) -> anyhow::Result<Option<Config>> {
    match source.read(CONFIG_PATH) {
        Ok(bytes) => {
            let text = String::from_utf8(bytes)?;
            let config = Config::parse(&text)
                .with_context(|| format!("invalid {CONFIG_PATH} at merge-base {merge_base_rev}"))?;
            Ok(Some(config))
        }
        Err(e) if is_not_found(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Orders paths as the file tree lists them: directories first, then by name.
fn tree_order(a: &str, b: &str) -> Ordering {
    let (sa, sb): (Vec<&str>, Vec<&str>) = (a.split('/').collect(), b.split('/').collect());
    for i in 0..sa.len().min(sb.len()) {
        let (a_dir, b_dir) = (i + 1 < sa.len(), i + 1 < sb.len());
        if a_dir != b_dir {
            return b_dir.cmp(&a_dir);
        }
        match sa[i].cmp(sb[i]) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    sa.len().cmp(&sb.len())
}

enum TreeNode {
    Dir {
        name: String,
        children: Vec<TreeNode>,
    },
    File {
        name: String,
        item: usize,
    },
}

/// Builds the file tree from items sorted by [`tree_order`].
fn build_tree(items: &[Item]) -> Vec<TreeNode> {
    fn insert(nodes: &mut Vec<TreeNode>, segments: &[&str], item: usize) {
        if let [name] = segments {
            nodes.push(TreeNode::File {
                name: name.to_string(),
                item,
            });
            return;
        }
        let continues =
            matches!(nodes.last(), Some(TreeNode::Dir { name, .. }) if name == segments[0]);
        if !continues {
            nodes.push(TreeNode::Dir {
                name: segments[0].to_string(),
                children: Vec::new(),
            });
        }
        if let Some(TreeNode::Dir { children, .. }) = nodes.last_mut() {
            insert(children, &segments[1..], item);
        }
    }
    let mut root = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let segments: Vec<&str> = item.path().split('/').collect();
        insert(&mut root, &segments, i);
    }
    root
}

fn tree_html(nodes: &[TreeNode], items: &[Item]) -> String {
    let mut out = String::from("<ul>");
    for node in nodes {
        match node {
            TreeNode::Dir { name, children } => out.push_str(&format!(
                "<li><details open><summary class=\"dir\">{}</summary>{}</details></li>",
                esc(name),
                tree_html(children, items)
            )),
            TreeNode::File { name, item } => out.push_str(&format!(
                "<li><a href=\"#d{}\"><span class=\"fn\">{}</span>{}</a></li>",
                item + 1,
                esc(name),
                icon(items[*item].status)
            )),
        }
    }
    out.push_str("</ul>");
    out
}

/// Writes the diff HTML (FR-108..120). Ok(()) also when nothing changed (no file written).
pub fn run(repo_root: &Path, config: &Config, opts: &DiffOptions) -> anyhow::Result<()> {
    let out: PathBuf = repo_root
        .join(opts.out.as_deref().unwrap_or(DEFAULT_OUT))
        .components()
        .collect();
    check_out_path(&out)?;
    if config.index.is_empty() {
        anyhow::bail!("index is not configured in .miwa/config.yaml");
    }
    let link_base = opts
        .link_base
        .as_deref()
        .map(normalize_link_base)
        .transpose()?;
    if let Some(label) = &opts.head_label {
        if label.chars().any(char::is_control) {
            anyhow::bail!("--head-label must not contain line breaks or control characters");
        }
    }

    let base_ref = opts
        .base_ref
        .clone()
        .unwrap_or_else(|| config.diff_base.clone());
    let merge_base_rev = merge_base(repo_root, &base_ref)?;
    let base_source = GitTreeSource::new(repo_root, &merge_base_rev)?;
    let head_source = FsSource::new(repo_root);
    let base = match base_config(&base_source, &merge_base_rev)? {
        Some(cfg) => Side::load(
            'b',
            &format!("merge-base {merge_base_rev}"),
            &base_source,
            &cfg,
        )?,
        None => Side::new(
            'b',
            &merge_base_rev,
            &base_source,
            SideModel::default(),
            Vec::new(),
        ),
    };
    let head = Side::load('h', "working tree", &head_source, config)?;

    let mut items = pair::pair_documents(&base, &head)?;
    items.extend(pair::pair_indexes(&base, &head));
    base.images.finish()?;
    head.images.finish()?;
    if items.is_empty() {
        return Ok(());
    }
    items.sort_by(|a, b| tree_order(a.path(), b.path()).then_with(|| a.status.cmp(&b.status)));

    let mut base_anchors = HashMap::new();
    let mut head_anchors = HashMap::new();
    let mut base_frameless = HashSet::new();
    let mut head_frameless = HashSet::new();
    for (i, item) in items.iter().enumerate() {
        if let Some(p) = &item.base {
            base_anchors.insert(p.clone(), format!("d{}", i + 1));
        }
        if let Some(p) = &item.head {
            head_anchors.insert(p.clone(), format!("d{}", i + 1));
        }
        if item.frameless {
            base_frameless.extend(item.base.clone());
            head_frameless.extend(item.head.clone());
        }
    }
    let ctx = Ctx {
        base: &base,
        head: &head,
        base_anchors,
        head_anchors,
        base_frameless,
        head_frameless,
        link_base: link_base.as_deref(),
    };
    let frames: String = items
        .iter()
        .enumerate()
        .map(|(i, item)| render_frame(&ctx, item, &format!("d{}", i + 1)))
        .collect();

    base.images.finish()?;
    head.images.finish()?;

    let site_title = config
        .site
        .as_ref()
        .and_then(|s| s.title())
        .map(str::to_string)
        .unwrap_or_else(|| {
            repo_root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let head_label = opts
        .head_label
        .clone()
        .unwrap_or_else(|| current_branch(repo_root));
    let head_rev = head_sha(repo_root)?;
    let html = document(
        &Page {
            site_title: &site_title,
            base_ref: &base_ref,
            base_sha: short(&merge_base_rev),
            head_label: &head_label,
            head_sha: short(&head_rev),
        },
        &items,
        &frames,
    );

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&out, html).with_context(|| format!("failed to write {}", out.display()))
}

/// The values of the page header.
struct Page<'a> {
    site_title: &'a str,
    base_ref: &'a str,
    base_sha: &'a str,
    head_label: &'a str,
    head_sha: &'a str,
}

fn document(page: &Page, items: &[Item], frames: &str) -> String {
    let counts: String = [Status::Mod, Status::Add, Status::Del, Status::Move]
        .into_iter()
        .filter_map(|status| {
            let n = items.iter().filter(|i| i.status == status).count();
            (n > 0).then(|| format!("<span>{}{n}</span>", icon(status)))
        })
        .collect();
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">\n\
<meta name=\"referrer\" content=\"no-referrer\">\n\
<title>{base} → {head} — {site}</title>\n<style>{style}</style>\n</head>\n<body>\n\
<header class=\"top\">{mark}<span class=\"site\">{site}</span>\
<span class=\"cmp\">{base} ({bsha}) → {head} ({hsha})</span><span class=\"counts\">{counts}</span></header>\n\
<div class=\"layout\">\n<aside aria-label=\"ファイル一覧\"><div class=\"tree\">{tree}</div></aside>\n<main>\n{frames}</main>\n</div>\n</body>\n</html>\n",
        base = esc(page.base_ref),
        head = esc(page.head_label),
        site = esc(page.site_title),
        bsha = esc(page.base_sha),
        hsha = esc(page.head_sha),
        style = style::STYLE,
        mark = style::MARK,
        tree = tree_html(&build_tree(items), items),
    )
}
