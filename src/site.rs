//! `miwa site`: static HTML output for reading the documents.
//!
//! The site mirrors the index tree: every published page is a document body
//! (or an index page) rendered from its Markdown source, with navigation built
//! from the index structure (FR-089 .. FR-107).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use pulldown_cmark::{CowStr, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::config::{is_under, join_under, normalize_dir, Config};
use crate::index::{
    collect_documents, document_title, normalize_description, IndexTree, ParsedText,
};
use crate::markdown::{parent_dir, uri_scheme};
use crate::model::{EdgeKind, Graph, NodeKind};
use crate::source::{FsSource, Source};

/// A relative link destination resolved against the source file's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedLink {
    /// Repository-relative path (normalized, percent-decoded).
    pub path: String,
    /// Fragment after `#`, if any.
    pub fragment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A req node (`type: Requirement`); lists the docs referencing it.
    Req,
    /// Any other document (including doc nodes). Whether an id is shown is
    /// decided by `Page::id`, not by the kind.
    Plain,
    /// An `index.md` under an index root.
    Index,
}

/// Classification of a typed document, by its front matter `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DocClass {
    /// `type: BusinessNeed`.
    Doc,
    /// `type: Requirement`.
    Req,
    /// Any other `type` (and index pages).
    Other,
}

impl DocClass {
    fn of_type(kind: Option<&str>) -> DocClass {
        match kind {
            Some("BusinessNeed") => DocClass::Doc,
            Some("Requirement") => DocClass::Req,
            _ => DocClass::Other,
        }
    }
}

pub(crate) struct Page {
    pub(crate) kind: Kind,
    pub(crate) id: Option<String>,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) body: String,
    pub(crate) class: DocClass,
    /// The front matter `id` (also for documents that are not graph nodes);
    /// unlike `id` it is never shown by `site`.
    pub(crate) front_id: Option<String>,
}

/// What rendering one page produced besides its HTML.
#[derive(Default)]
pub(crate) struct Rendered {
    warnings: Vec<String>,
    images: BTreeSet<String>,
}

pub(crate) struct Env<'a> {
    pages: &'a BTreeMap<String, Page>,
    files: &'a HashSet<String>,
    /// Normalized `docs` and `requirements` roots (images are copied from here).
    media_roots: &'a [String],
}

const STYLE: &str = "body{max-width:48rem;margin:0 auto;padding:1rem;font-family:system-ui,-apple-system,sans-serif;line-height:1.7;color:#222;background:#fff}\
nav{font-size:.9rem;margin-bottom:1.5rem;color:#666}\
a{color:#0a58ca}\
img{max-width:100%}\
pre,code{background:#f5f5f5}pre{padding:.75rem;overflow-x:auto}\
table{border-collapse:collapse}th,td{border:1px solid #ccc;padding:.25rem .6rem}\
.id{font-family:monospace;color:#666;margin:0}\
.description{color:#555}";

/// Writes the site into `out` and returns warning lines (without trailing newline).
pub fn run(repo_root: &Path, config: &Config, out: &Path) -> anyhow::Result<Vec<String>> {
    if config.index.is_empty() {
        anyhow::bail!("index is not configured in .miwa/config.yaml");
    }
    // Normalize a trailing separator and `.` so the symlink check sees the entry itself.
    let out_dir: PathBuf = repo_root.join(out).components().collect();
    check_out_dir(&out_dir)?;

    let source = FsSource::new(repo_root);
    let model = load_side(&source, config)?;
    let pages = &model.pages;
    let roots = &model.roots;

    let mut warnings: Vec<String> = Vec::new();
    for root in roots {
        let root_index = join_under(root, "index.md");
        if !matches!(pages.get(&root_index), Some(p) if p.kind == Kind::Index) {
            warnings.push(format!(
                "warning: {}: missing index",
                escape_controls(&root_index)
            ));
        }
    }

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
    let site_description = config
        .site
        .as_ref()
        .and_then(|s| s.description())
        .map(str::to_string);

    let env = model.env();

    let (output, images) = render_pages(
        &env,
        &model.trees,
        &model.referrers,
        roots,
        &site_title,
        site_description.as_deref(),
        &mut warnings,
    );
    write_output(&out_dir, &source, &output, &images)?;
    Ok(warnings)
}

/// One side of a comparison: the published pages of a repository snapshot
/// with the index structure, built by [`load_side`].
#[derive(Default)]
pub(crate) struct SideModel {
    /// Documents and index pages by repository-relative path.
    pub(crate) pages: BTreeMap<String, Page>,
    /// The index tree of each root.
    pub(crate) trees: Vec<(String, IndexTree)>,
    /// doc -> req references, keyed by the req page path.
    pub(crate) referrers: BTreeMap<String, BTreeSet<String>>,
    /// Normalized index roots.
    pub(crate) roots: Vec<String>,
    /// Files of the snapshot (excluded directories removed).
    pub(crate) files: Vec<String>,
    file_set: HashSet<String>,
    /// Normalized `docs` and `requirements` roots (images are read from here).
    media_roots: Vec<String>,
}

impl SideModel {
    pub(crate) fn env(&self) -> Env<'_> {
        Env {
            pages: &self.pages,
            files: &self.file_set,
            media_roots: &self.media_roots,
        }
    }

    /// The repository-relative paths of the images the page at `path`
    /// references and that are published (FR-100's conditions).
    pub(crate) fn referenced_images(&self, path: &str, body: &str) -> BTreeSet<String> {
        let found = std::cell::RefCell::new(BTreeSet::new());
        let mut record = |image: &str| {
            found.borrow_mut().insert(image.to_string());
            Some(image.to_string())
        };
        let anchors = HashMap::new();
        let omit_body = HashSet::new();
        let mut mode = Mode::Diff(DiffMode {
            own: "",
            side: None,
            anchors: &anchors,
            omit_body: &omit_body,
            link_base: None,
            image_src: &mut record,
        });
        rewrite_events(
            &self.env(),
            path,
            body,
            "",
            &mut mode,
            &mut Rendered::default(),
        );
        found.into_inner()
    }
}

/// Reads the published pages of `source` under `config`: documents and index
/// pages under the index roots plus doc / req nodes, and the index tree of
/// each root.
pub(crate) fn load_side(source: &dyn Source, config: &Config) -> anyhow::Result<SideModel> {
    let graph = crate::graph::build_graph(source, config)?;
    let files: Vec<String> = source
        .list_files()?
        .into_iter()
        .filter(|p| !crate::source::is_under_dir_named(p, &config.exclude))
        .collect();
    let roots: Vec<String> = config.index.iter().map(|r| normalize_dir(r)).collect();

    // Sources of every candidate page (UTF-8 lossy, like `build_graph`).
    let node_paths: BTreeSet<&str> = graph
        .nodes
        .values()
        .filter(|n| matches!(n.key.kind, NodeKind::Doc | NodeKind::Req))
        .map(|n| n.path.as_str())
        .collect();
    let mut texts: BTreeMap<String, ParsedText> = BTreeMap::new();
    for path in &files {
        let wanted = path.ends_with(".md")
            && (node_paths.contains(path.as_str()) || roots.iter().any(|r| is_under(path, r)));
        if wanted {
            let text = String::from_utf8_lossy(
                &source
                    .read(path)
                    .with_context(|| format!("failed to read {path}"))?,
            )
            .into_owned();
            let front_matter = crate::frontmatter::parse(&text);
            texts.insert(path.clone(), ParsedText { text, front_matter });
        }
    }

    let (pages, trees) = collect_pages(&graph, &texts, &roots);
    let referrers = collect_referrers(&graph);

    let media_roots: Vec<String> = config
        .docs
        .iter()
        .chain(config.requirements.iter())
        .map(|d| normalize_dir(d))
        .collect();
    let file_set: HashSet<String> = files.iter().cloned().collect();
    Ok(SideModel {
        pages,
        trees,
        referrers,
        roots,
        files,
        file_set,
        media_roots,
    })
}

/// A `BusinessNeed` / `Requirement` file under `docs` / `requirements` that
/// `model` does not publish (for example one whose `id` is missing or invalid
/// and that lies outside the index roots), with its `id` as written (before validation).
pub(crate) struct Candidate {
    pub(crate) path: String,
    pub(crate) class: DocClass,
    pub(crate) id: Option<String>,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) body: String,
}

/// The doc / req files under `docs` / `requirements` that `model` does not publish.
pub(crate) fn unpublished_candidates(
    source: &dyn Source,
    config: &Config,
    model: &SideModel,
) -> anyhow::Result<Vec<Candidate>> {
    let dirs: Vec<String> = config
        .docs
        .iter()
        .chain(config.requirements.iter())
        .map(|d| normalize_dir(d))
        .collect();
    let mut out = Vec::new();
    for path in &model.files {
        let reserved = matches!(path.rsplit('/').next(), Some("index.md" | "log.md"));
        if !path.ends_with(".md")
            || reserved
            || model.pages.contains_key(path)
            || !dirs.iter().any(|d| is_under(path, d))
        {
            continue;
        }
        let text = String::from_utf8_lossy(
            &source
                .read(path)
                .with_context(|| format!("failed to read {path}"))?,
        )
        .into_owned();
        let Ok(Some(fm)) = crate::frontmatter::parse(&text) else {
            continue;
        };
        let class = DocClass::of_type(fm.kind.as_deref());
        if class == DocClass::Other {
            continue;
        }
        out.push(Candidate {
            path: path.clone(),
            class,
            id: fm.id.clone().filter(|id| !id.trim().is_empty()),
            title: document_title(path, &fm),
            description: normalize_description(&fm),
            body: fm.body.clone(),
        });
    }
    Ok(out)
}

/// Pages: documents and index pages under the roots, plus doc / req nodes,
/// and the index tree of each root.
fn collect_pages(
    graph: &Graph,
    texts: &BTreeMap<String, ParsedText>,
    roots: &[String],
) -> (BTreeMap<String, Page>, Vec<(String, IndexTree)>) {
    let mut pages: BTreeMap<String, Page> = BTreeMap::new();
    let mut trees: Vec<(String, IndexTree)> = Vec::new();
    for root in roots {
        let found = collect_documents(root, texts);
        for d in found.documents {
            let parsed = &texts[&d.path];
            let body = body_of(&parsed.text);
            let (class, front_id) = class_and_id(parsed);
            pages.insert(
                d.path,
                Page {
                    kind: Kind::Plain,
                    id: None,
                    title: d.title,
                    description: d.description,
                    body,
                    class,
                    front_id,
                },
            );
        }
        let index_paths: Vec<String> = found.indexes.iter().map(|m| m.path.clone()).collect();
        for m in found.indexes {
            let body = body_of(&texts[&m.path].text);
            pages.insert(
                m.path,
                Page {
                    kind: Kind::Index,
                    id: None,
                    title: m.title,
                    description: m.description,
                    body,
                    class: DocClass::Other,
                    front_id: None,
                },
            );
        }
        trees.push((root.clone(), IndexTree::new(root, index_paths)));
    }
    // A graph node overrides a collect_documents entry for the same path.
    for node in graph.nodes.values() {
        let (kind, class) = match node.key.kind {
            NodeKind::Doc => (Kind::Plain, DocClass::Doc),
            NodeKind::Req => (Kind::Req, DocClass::Req),
            NodeKind::Test => continue,
        };
        let Some(parsed) = texts.get(&node.path) else {
            continue;
        };
        let Ok(Some(fm)) = &parsed.front_matter else {
            continue;
        };
        pages.insert(
            node.path.clone(),
            Page {
                kind,
                id: Some(node.key.id.clone()),
                title: document_title(&node.path, fm),
                description: normalize_description(fm),
                body: body_of(&parsed.text),
                class,
                front_id: Some(node.key.id.clone()),
            },
        );
    }
    (pages, trees)
}

/// The class and the (non-blank) front matter `id` of a parsed document.
pub(crate) fn class_and_id(parsed: &ParsedText) -> (DocClass, Option<String>) {
    match &parsed.front_matter {
        Ok(Some(fm)) => (
            DocClass::of_type(fm.kind.as_deref()),
            fm.id.clone().filter(|id| !id.trim().is_empty()),
        ),
        _ => (DocClass::Other, None),
    }
}

/// doc -> req references, keyed by the req page path.
fn collect_referrers(graph: &Graph) -> BTreeMap<String, BTreeSet<String>> {
    let mut referrers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in graph.edges.values() {
        if edge.kind != EdgeKind::DocReq {
            continue;
        }
        if let (Some(doc), Some(req)) = (graph.nodes.get(&edge.from), graph.nodes.get(&edge.to)) {
            referrers
                .entry(req.path.clone())
                .or_default()
                .insert(doc.path.clone());
        }
    }
    referrers
}

/// Renders every page and the top page; returns the files by output path and
/// the set of images to copy.
fn render_pages(
    env: &Env,
    trees: &[(String, IndexTree)],
    referrers: &BTreeMap<String, BTreeSet<String>>,
    roots: &[String],
    site_title: &str,
    site_description: Option<&str>,
    warnings: &mut Vec<String>,
) -> (BTreeMap<String, Vec<u8>>, BTreeSet<String>) {
    let pages = env.pages;
    let mut output: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut images: BTreeSet<String> = BTreeSet::new();
    for (path, page) in pages {
        let from_dir = parent_dir(path);
        let mut nav = vec![(
            relative_path(from_dir, "index.html"),
            site_title.to_string(),
        )];
        if let Some(index_path) = nav_index_of(path, page, trees, pages) {
            nav.push((
                relative_path(from_dir, &html_path(&index_path)),
                pages[&index_path].title.clone(),
            ));
        }
        let mut rendered = Rendered::default();
        let html = render_page(env, path, page, &nav, referrers.get(path), &mut rendered);
        warnings.extend(rendered.warnings);
        images.extend(rendered.images);
        output.insert(html_path(path), html.into_bytes());
    }
    output.insert(
        "index.html".to_string(),
        top_page(roots, pages, site_title, site_description).into_bytes(),
    );
    (output, images)
}

/// Writes the pages, then copies the images one at a time (never all held in
/// memory).
fn write_output(
    out_dir: &Path,
    source: &FsSource,
    output: &BTreeMap<String, Vec<u8>>,
    images: &BTreeSet<String>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let write_file = |rel: &str, bytes: &[u8]| -> anyhow::Result<()> {
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(&dest, bytes).with_context(|| format!("failed to write {}", dest.display()))
    };
    for (rel, bytes) in output {
        write_file(rel, bytes)?;
    }
    for image in images {
        let bytes = source
            .read(image)
            .with_context(|| format!("failed to read {image}"))?;
        write_file(image, &bytes)?;
    }
    Ok(())
}

/// The output directory must be absent or an empty directory.
fn check_out_dir(out_dir: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(out_dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            anyhow::bail!("output path {} is a symbolic link", out_dir.display())
        }
        Ok(meta) if meta.is_dir() => {
            let mut entries = std::fs::read_dir(out_dir).with_context(|| {
                format!("failed to inspect output directory {}", out_dir.display())
            })?;
            if entries.next().is_some() {
                anyhow::bail!(
                    "output directory {} is not empty; empty it first",
                    out_dir.display()
                );
            }
            Ok(())
        }
        Ok(_) => anyhow::bail!(
            "output path {} exists and is not a directory",
            out_dir.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context(format!(
            "failed to inspect output directory {}",
            out_dir.display()
        ))),
    }
}

/// The body without the front matter. An unterminated front matter gives an
/// empty body (only index pages get here in that case; FR-102).
fn body_of(text: &str) -> String {
    match crate::frontmatter::split(text) {
        Ok(Some((_, body))) => body.to_string(),
        Err(_) => String::new(),
        Ok(None) => text.to_string(),
    }
}

/// The published index page closest to `path` (FR-104), if any.
fn nav_index_of(
    path: &str,
    page: &Page,
    trees: &[(String, IndexTree)],
    pages: &BTreeMap<String, Page>,
) -> Option<String> {
    let (_, tree) = trees.iter().find(|(root, _)| is_under(path, root))?;
    let candidate = if page.kind == Kind::Index {
        tree.parent_of(path)?
    } else {
        tree.owner_of(path)
    };
    match pages.get(&candidate) {
        Some(p) if p.kind == Kind::Index => Some(candidate),
        _ => None,
    }
}

/// `docs/a.md` -> `docs/a.html`.
fn html_path(md_path: &str) -> String {
    format!("{}.html", md_path.strip_suffix(".md").unwrap_or(md_path))
}

/// The path from directory `from_dir` to the repository-relative file `to`.
fn relative_path(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = to.split('/').collect();
    let limit = from.len().min(to.len() - 1);
    let common = (0..limit).take_while(|&i| from[i] == to[i]).count();
    let mut out = "../".repeat(from.len() - common);
    out.push_str(&to[common..].join("/"));
    out
}

pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// GitHub-compatible heading id for the rendered plain text of a heading
/// (empty when nothing remains).
pub(crate) fn heading_slug(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// Resolves a relative link / image destination from a file in `source_dir`
/// (repository-relative directory, `""` for the root). `None` when the
/// destination is empty (after dropping `?...`) or escapes the repository.
pub(crate) fn resolve_link_path(source_dir: &str, dest: &str) -> Option<ResolvedLink> {
    let (path_part, fragment) = match dest.split_once('#') {
        Some((p, f)) => (p, Some(f.to_string())),
        None => (dest, None),
    };
    let path_part = path_part.split('?').next().unwrap_or("");
    if path_part.is_empty() {
        return None;
    }
    let decoded = crate::markdown::percent_decode(path_part);
    // `normalize_relative` drops the last segment of its base (a file name).
    let base = if source_dir.is_empty() {
        String::new()
    } else {
        format!("{source_dir}/_")
    };
    let path = crate::markdown::normalize_relative(&base, &decoded)?;
    Some(ResolvedLink { path, fragment })
}

/// RFC 3986 pchar minus pct-encoded (`@` and `:` included).
fn is_pchar_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

fn push_encoded(out: &mut String, b: u8) {
    out.push_str(&format!("%{b:02X}"));
}

fn encode_fragment(fragment: &str) -> String {
    let bytes = fragment.as_bytes();
    let mut out = String::with_capacity(fragment.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push_str(&fragment[i..i + 3]);
            i += 3;
            continue;
        }
        if is_pchar_byte(b) || b == b'/' || b == b'?' {
            out.push(b as char);
        } else {
            push_encoded(&mut out, b);
        }
        i += 1;
    }
    out
}

/// Builds an href from a resolved path: each segment percent-encoded except
/// RFC 3986 pchar, a `:` in the first segment encoded as `%3A`, fragment
/// appended after `#`.
pub(crate) fn encode_href(path: &str, fragment: Option<&str>) -> String {
    let mut out = String::with_capacity(path.len());
    for (i, segment) in path.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        for &b in segment.as_bytes() {
            if b == b':' && i == 0 {
                out.push_str("%3A");
            } else if is_pchar_byte(b) {
                out.push(b as char);
            } else {
                push_encoded(&mut out, b);
            }
        }
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(&encode_fragment(f));
    }
    out
}

/// What a link destination points at.
enum LinkTarget {
    /// A `#fragment` of the same page (the text after `#`).
    Fragment(String),
    /// An `http:` / `https:` / `mailto:` destination, kept as written.
    External(String),
    /// A published page (its `.md` path) with an optional fragment.
    Page {
        md: String,
        fragment: Option<String>,
    },
    Unpublished,
}

/// What an image destination points at.
enum ImageTarget {
    /// An `https:` image, referenced as written.
    External(String),
    /// A published image file.
    Local {
        path: String,
        fragment: Option<String>,
    },
    Unpublished,
}

impl Env<'_> {
    /// The `.md` path of the page a resolved path publishes as: a published
    /// `.md` page, or the published `index.md` of a directory.
    fn target_md(&self, path: &str) -> Option<String> {
        if path.ends_with(".md") && self.pages.contains_key(path) {
            return Some(path.to_string());
        }
        let index = format!("{path}/index.md");
        match self.pages.get(&index) {
            Some(p) if p.kind == Kind::Index => Some(index),
            _ => None,
        }
    }

    /// Classifies a link destination of the file `src_path` (FR-098, FR-099).
    fn link_target(&self, src_path: &str, dest: &str) -> LinkTarget {
        if let Some(rest) = dest.strip_prefix('#') {
            return LinkTarget::Fragment(rest.to_string());
        }
        if let Some(scheme) = uri_scheme(dest) {
            let ok = ["http", "https", "mailto"]
                .iter()
                .any(|s| scheme.eq_ignore_ascii_case(s));
            return if ok {
                LinkTarget::External(dest.to_string())
            } else {
                LinkTarget::Unpublished
            };
        }
        if dest.starts_with('/') {
            return LinkTarget::Unpublished;
        }
        let Some(resolved) = resolve_link_path(parent_dir(src_path), dest) else {
            return LinkTarget::Unpublished;
        };
        match self.target_md(&resolved.path) {
            Some(md) => LinkTarget::Page {
                md,
                fragment: resolved.fragment,
            },
            None => LinkTarget::Unpublished,
        }
    }

    /// The href for a link destination, or `None` when it is not published
    /// (FR-098, FR-099).
    fn link_href(&self, src_path: &str, dest: &str) -> Option<String> {
        match self.link_target(src_path, dest) {
            LinkTarget::Fragment(rest) => Some(format!("#{}", encode_fragment(&rest))),
            LinkTarget::External(url) => Some(url),
            LinkTarget::Page { md, fragment } => Some(encode_href(
                &relative_path(parent_dir(src_path), &html_path(&md)),
                fragment.as_deref(),
            )),
            LinkTarget::Unpublished => None,
        }
    }

    /// Classifies an image destination of the file `src_path` (FR-100).
    fn image_target(&self, src_path: &str, dest: &str) -> ImageTarget {
        if let Some(scheme) = uri_scheme(dest) {
            return if scheme.eq_ignore_ascii_case("https") {
                ImageTarget::External(dest.to_string())
            } else {
                ImageTarget::Unpublished
            };
        }
        if dest.starts_with('/') {
            return ImageTarget::Unpublished;
        }
        let Some(resolved) = resolve_link_path(parent_dir(src_path), dest) else {
            return ImageTarget::Unpublished;
        };
        let Some((_, ext)) = resolved.path.rsplit_once('.') else {
            return ImageTarget::Unpublished;
        };
        let ext = ext.to_ascii_lowercase();
        if !["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
            return ImageTarget::Unpublished;
        }
        if !self.files.contains(&resolved.path)
            || !self.media_roots.iter().any(|r| is_under(&resolved.path, r))
        {
            return ImageTarget::Unpublished;
        }
        ImageTarget::Local {
            path: resolved.path,
            fragment: resolved.fragment,
        }
    }

    /// The `src` for an image destination, or `None` when it is not published
    /// (FR-100). Copied images are recorded in `images`.
    fn image_src(
        &self,
        src_path: &str,
        dest: &str,
        images: &mut BTreeSet<String>,
    ) -> Option<String> {
        match self.image_target(src_path, dest) {
            ImageTarget::External(url) => Some(url),
            ImageTarget::Local { path, fragment } => {
                let href = encode_href(
                    &relative_path(parent_dir(src_path), &path),
                    fragment.as_deref(),
                );
                images.insert(path);
                Some(href)
            }
            ImageTarget::Unpublished => None,
        }
    }
}

/// Removes every `<!--` ... `-->` span (to the end when unterminated).
fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start + 4..].find("-->") {
            Some(end) => rest = &rest[start + 4 + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Escapes control characters as `\u{1b}` (FR-099, FR-100).
fn escape_controls(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() {
                format!("\\u{{{:x}}}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn is_comment_html(s: &str) -> bool {
    s.trim_start().starts_with("<!--")
}

/// Concatenated text of an HTML block whose `Start` was just consumed (`i` is
/// the index after it); returns the text and the index after its `End`.
fn html_block_text(events: &[Event], mut i: usize) -> (String, usize) {
    let mut text = String::new();
    while i < events.len() {
        let piece = &events[i];
        i += 1;
        match piece {
            Event::End(TagEnd::HtmlBlock) => break,
            Event::Html(s) | Event::Text(s) => text.push_str(s),
            _ => {}
        }
    }
    (text, i)
}

/// Whether the first block is a level-1 heading with rendered content. HTML
/// comment blocks are not blocks and are skipped; a heading holding only
/// inline comments and whitespace is empty.
fn starts_with_h1(events: &[Event]) -> bool {
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::HtmlBlock) => {
                let (text, next) = html_block_text(events, i + 1);
                if !is_comment_html(&text) {
                    return false;
                }
                i = next;
                continue;
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let empty = events[i + 1..]
                    .iter()
                    .take_while(|e| !matches!(e, Event::End(TagEnd::Heading(_))))
                    .all(|e| match e {
                        Event::InlineHtml(s) => is_comment_html(s),
                        Event::Text(t) => t.trim().is_empty(),
                        _ => false,
                    });
                return *level == HeadingLevel::H1 && !empty;
            }
            _ => return false,
        }
    }
    false
}

/// How links, images, heading ids and footnotes of a body are rewritten.
pub(crate) enum Mode<'a> {
    /// `miwa site`: relative links between the generated pages, images copied.
    Site,
    /// `miwa site --diff`: one page per side, ids prefixed per document and side.
    Diff(DiffMode<'a>),
}

pub(crate) struct DiffMode<'a> {
    /// Anchor of the document being rendered (`d3`).
    pub(crate) own: &'a str,
    /// `b` (base) or `h` (head); `None` renders the comparison key, whose ids
    /// carry no side.
    pub(crate) side: Option<char>,
    /// Anchor of each document in the diff, by path on this side.
    pub(crate) anchors: &'a HashMap<String, String>,
    /// Documents of the diff whose frame has no content (moved only): links
    /// into them point to the frame, never to a heading.
    pub(crate) omit_body: &'a HashSet<String>,
    /// Base URL (with a trailing `/`) for links to documents outside the diff.
    pub(crate) link_base: Option<&'a str>,
    /// The `src` for a published image file (by path), or `None` to drop it.
    pub(crate) image_src: &'a mut dyn FnMut(&str) -> Option<String>,
}

/// The id prefix of the document `anchor` on the side `side`.
pub(crate) fn id_prefix(anchor: &str, side: Option<char>) -> String {
    match side {
        Some(c) => format!("{anchor}-{c}-"),
        None => format!("{anchor}-"),
    }
}

/// What a top-level list item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListKind {
    Unordered,
    /// The number of this item.
    Ordered(u64),
}

/// A top-level block of a rewritten body: a range of events.
#[derive(Debug, Clone)]
pub(crate) struct BlockSpan {
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) kind: crate::diff::BlockKind,
    /// Set for a list item (the span covers the item, not the list).
    pub(crate) list: Option<ListKind>,
}

/// The rewritten events of a body and where its top-level blocks lie.
pub(crate) struct Rewritten<'a> {
    pub(crate) events: Vec<Event<'a>>,
    pub(crate) blocks: Vec<BlockSpan>,
    /// Whether the first block is a level-1 heading with rendered content.
    pub(crate) first_is_h1: bool,
}

fn block_kind(tag: &Tag) -> crate::diff::BlockKind {
    use crate::diff::BlockKind;
    match tag {
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::Heading { .. } => BlockKind::Heading,
        Tag::CodeBlock(_) => BlockKind::Code,
        Tag::Table(_) => BlockKind::Table,
        Tag::BlockQuote(_) => BlockKind::Quote,
        Tag::FootnoteDefinition(_) => BlockKind::Footnote,
        _ => BlockKind::Other,
    }
}

/// Converts the Markdown `body` of the page at `src_path` to HTML. When the
/// first block is a level-1 heading, `meta` (id and summary) is placed right
/// after it and the second value is `true`.
fn render_body(
    env: &Env,
    src_path: &str,
    body: &str,
    meta: &str,
    rendered: &mut Rendered,
) -> (String, bool) {
    let r = rewrite_events(env, src_path, body, meta, &mut Mode::Site, rendered);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, r.events.into_iter());
    (html, r.first_is_h1)
}

/// Resolves a link destination to the href to write, or `None` to drop the
/// link and keep its text.
fn mode_link_href(
    env: &Env,
    mode: &Mode,
    src_path: &str,
    dest: &str,
    rendered: &mut Rendered,
) -> Option<String> {
    let Mode::Diff(diff) = mode else {
        let href = env.link_href(src_path, dest);
        if href.is_none() {
            rendered.warnings.push(format!(
                "warning: {}: unpublished link: {}",
                escape_controls(src_path),
                escape_controls(dest)
            ));
        }
        return href;
    };
    match env.link_target(src_path, dest) {
        LinkTarget::Fragment(rest) if rest.is_empty() => Some("#".to_string()),
        LinkTarget::Fragment(rest) => Some(format!(
            "#{}{}",
            id_prefix(diff.own, diff.side),
            encode_fragment(&rest)
        )),
        LinkTarget::External(url) => Some(url),
        LinkTarget::Page { md, fragment } => match diff.anchors.get(&md) {
            Some(anchor) => Some(match fragment {
                Some(f) if !f.is_empty() && !diff.omit_body.contains(&md) => {
                    format!("#{}{}", id_prefix(anchor, diff.side), encode_fragment(&f))
                }
                _ => format!("#{anchor}"),
            }),
            None => diff
                .link_base
                .map(|base| format!("{base}{}", encode_href(&md, fragment.as_deref()))),
        },
        LinkTarget::Unpublished => None,
    }
}

/// Resolves an image destination to the `src` to write, or `None` to drop the
/// image and keep its alt text.
fn mode_image_src(
    env: &Env,
    mode: &mut Mode,
    src_path: &str,
    dest: &str,
    rendered: &mut Rendered,
) -> Option<String> {
    let Mode::Diff(diff) = mode else {
        let src = env.image_src(src_path, dest, &mut rendered.images);
        if src.is_none() {
            rendered.warnings.push(format!(
                "warning: {}: unpublished image: {}",
                escape_controls(src_path),
                escape_controls(dest)
            ));
        }
        return src;
    };
    match env.image_target(src_path, dest) {
        ImageTarget::External(url) => Some(url),
        ImageTarget::Local { path, .. } => (diff.image_src)(&path),
        ImageTarget::Unpublished => None,
    }
}

/// Parses the Markdown `body` of the page at `src_path` and rewrites links,
/// images, heading ids, comments and raw HTML. Top-level blocks are reported
/// as ranges of the rewritten events (list items as blocks of their own).
pub(crate) fn rewrite_events<'a>(
    env: &Env,
    src_path: &str,
    body: &'a str,
    meta: &str,
    mode: &mut Mode,
    rendered: &mut Rendered,
) -> Rewritten<'a> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let events: Vec<Event> = Parser::new_ext(body, options).collect();

    // Footnotes are rendered by hand in the diff (their ids carry a prefix).
    let (id_pfx, footnote_prefix) = match mode {
        Mode::Site => (String::new(), ""),
        Mode::Diff(d) => (id_prefix(d.own, d.side), "fn-"),
    };
    let mut used_ids: HashSet<String> = HashSet::new();
    // Next suffix to try per slug (avoids rescanning `slug-1`, `slug-2`, ...).
    let mut next_suffix: HashMap<String, usize> = HashMap::new();
    for ev in &events {
        match ev {
            Event::FootnoteReference(label) | Event::Start(Tag::FootnoteDefinition(label)) => {
                used_ids.insert(format!("{footnote_prefix}{label}"));
            }
            _ => {}
        }
    }

    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut blocks: Vec<BlockSpan> = Vec::new();
    // The open top-level block (start index, kind) and the open top-level list
    // (ordered start, items so far).
    let mut open_block: Option<(usize, crate::diff::BlockKind, Option<ListKind>)> = None;
    let mut open_list: Option<(Option<u64>, u64)> = None;
    // Whether each open link / image is kept (an unpublished one loses its tags).
    let mut kept: Vec<bool> = Vec::new();
    let mut depth = 0usize;
    let first_is_h1 = starts_with_h1(&events);
    let mut injected = false;
    let mut i = 0;
    while i < events.len() {
        let ev = events[i].clone();
        i += 1;
        let before = out.len();
        let depth_before = depth;
        // Block bookkeeping needs the event's shape before it is consumed.
        let opens = match &ev {
            Event::Start(Tag::HtmlBlock) => None,
            Event::Start(tag) => Some(tag.clone()),
            _ => None,
        };
        let leaf_kind = match &ev {
            Event::Start(Tag::HtmlBlock) => Some(crate::diff::BlockKind::Html),
            Event::Rule => Some(crate::diff::BlockKind::Rule),
            Event::Start(_) | Event::End(_) => None,
            _ => Some(crate::diff::BlockKind::Other),
        };
        let closes = matches!(ev, Event::End(_));
        match ev {
            Event::Start(Tag::HtmlBlock) => {
                let (text, next) = html_block_text(&events, i);
                i = next;
                if is_comment_html(&text) {
                    continue;
                }
                if matches!(mode, Mode::Diff(_)) {
                    // The renderer escapes the text, which then takes part in
                    // the character-level diff.
                    out.push(Event::Html("<pre>".into()));
                    out.push(Event::Text(strip_comments(&text).into()));
                    out.push(Event::Html("</pre>\n".into()));
                } else {
                    out.push(Event::Html(
                        format!("<pre>{}</pre>\n", esc(&strip_comments(&text))).into(),
                    ));
                }
            }
            Event::InlineHtml(s) => {
                if !is_comment_html(&s) {
                    out.push(Event::Text(s));
                }
            }
            Event::FootnoteReference(label) if matches!(mode, Mode::Diff(_)) => {
                let id = format!("{id_pfx}fn-{label}");
                out.push(Event::Html(
                    format!(
                        "<sup class=\"footnote-reference\"><a href=\"#{}\">{}</a></sup>",
                        esc(&encode_fragment(&id)),
                        esc(&label)
                    )
                    .into(),
                ));
            }
            Event::Start(Tag::FootnoteDefinition(label)) if matches!(mode, Mode::Diff(_)) => {
                depth += 1;
                out.push(Event::Html(
                    format!(
                        "<div class=\"footnote-definition\" id=\"{}\"><sup class=\"footnote-definition-label\">{}</sup>",
                        esc(&format!("{id_pfx}fn-{label}")),
                        esc(&label)
                    )
                    .into(),
                ));
            }
            Event::End(TagEnd::FootnoteDefinition) if matches!(mode, Mode::Diff(_)) => {
                depth = depth.saturating_sub(1);
                out.push(Event::Html("</div>\n".into()));
            }
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => {
                let mut plain = String::new();
                for inner in &events[i..] {
                    match inner {
                        Event::End(TagEnd::Heading(_)) => break,
                        Event::Text(t) | Event::Code(t) => plain.push_str(t),
                        _ => {}
                    }
                }
                let slug = heading_slug(plain.trim());
                let id = if slug.is_empty() {
                    None
                } else {
                    let mut candidate = slug.clone();
                    let next = next_suffix.entry(slug.clone()).or_insert(0);
                    while used_ids.contains(&candidate) {
                        *next += 1;
                        candidate = format!("{slug}-{next}");
                    }
                    used_ids.insert(candidate.clone());
                    Some(CowStr::from(format!("{id_pfx}{candidate}")))
                };
                depth += 1;
                out.push(Event::Start(Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                }));
            }
            Event::End(TagEnd::Heading(level)) => {
                depth = depth.saturating_sub(1);
                out.push(Event::End(TagEnd::Heading(level)));
                if depth == 0 && first_is_h1 && !injected && matches!(mode, Mode::Site) {
                    injected = true;
                    out.push(Event::Html(meta.to_string().into()));
                }
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                depth += 1;
                let dest = if link_type == LinkType::Email {
                    format!("mailto:{dest_url}")
                } else {
                    dest_url.to_string()
                };
                match mode_link_href(env, mode, src_path, &dest, rendered) {
                    Some(href) => {
                        kept.push(true);
                        out.push(Event::Start(Tag::Link {
                            link_type: if link_type == LinkType::Email {
                                LinkType::Inline
                            } else {
                                link_type
                            },
                            dest_url: href.into(),
                            title,
                            id,
                        }));
                    }
                    None => kept.push(false),
                }
            }
            Event::End(TagEnd::Link) => {
                depth = depth.saturating_sub(1);
                if kept.pop().expect("link/image end matches a start") {
                    out.push(Event::End(TagEnd::Link));
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                depth += 1;
                match mode_image_src(env, mode, src_path, &dest_url, rendered) {
                    Some(src) => {
                        kept.push(true);
                        out.push(Event::Start(Tag::Image {
                            link_type,
                            dest_url: src.into(),
                            title,
                            id,
                        }));
                    }
                    None => kept.push(false),
                }
            }
            Event::End(TagEnd::Image) => {
                depth = depth.saturating_sub(1);
                if kept.pop().expect("link/image end matches a start") {
                    out.push(Event::End(TagEnd::Image));
                }
            }
            Event::Start(tag) => {
                depth += 1;
                out.push(Event::Start(tag));
            }
            Event::End(tag) => {
                depth = depth.saturating_sub(1);
                out.push(Event::End(tag));
            }
            other => {
                out.push(other);
            }
        }

        // Top-level blocks: a tag opened at depth 0, or a leaf at depth 0; list
        // items (depth 1 inside a top-level list) are blocks of their own.
        if let Some(tag) = opens {
            match tag {
                Tag::List(start) if depth_before == 0 => open_list = Some((start, 0)),
                Tag::Item if depth_before == 1 && open_list.is_some() => {
                    let (start, index) = open_list.as_mut().expect("checked above");
                    let list = match start {
                        Some(s) => ListKind::Ordered(*s + *index),
                        None => ListKind::Unordered,
                    };
                    *index += 1;
                    open_block = Some((before, crate::diff::BlockKind::ListItem, Some(list)));
                }
                tag if depth_before == 0 => open_block = Some((before, block_kind(&tag), None)),
                _ => {}
            }
        } else if closes {
            let block_ends = depth == 0 && open_list.is_none() || depth == 1 && open_list.is_some();
            if depth == 0 {
                open_list = None;
            }
            if block_ends {
                if let Some((start, kind, list)) = open_block.take() {
                    blocks.push(BlockSpan {
                        range: start..out.len(),
                        kind,
                        list,
                    });
                }
            }
        } else if let (Some(kind), 0) = (leaf_kind, depth_before) {
            if out.len() > before {
                blocks.push(BlockSpan {
                    range: before..out.len(),
                    kind,
                    list: None,
                });
            }
        }
    }
    Rewritten {
        events: out,
        blocks,
        first_is_h1,
    }
}

fn meta_block(page: &Page) -> String {
    let mut meta = String::new();
    if let Some(id) = &page.id {
        meta.push_str(&format!("<p class=\"id\">{}</p>\n", esc(id)));
    }
    if let Some(d) = &page.description {
        meta.push_str(&format!("<p class=\"description\">{}</p>\n", esc(d)));
    }
    meta
}

fn render_page(
    env: &Env,
    path: &str,
    page: &Page,
    nav: &[(String, String)],
    referrers: Option<&BTreeSet<String>>,
    rendered: &mut Rendered,
) -> String {
    let meta = meta_block(page);
    let (body_html, uses_body_h1) = render_body(env, path, &page.body, &meta, rendered);
    let mut main = String::new();
    if !uses_body_h1 {
        main.push_str(&format!("<h1>{}</h1>\n{meta}", esc(&page.title)));
    }
    main.push_str(&body_html);
    if page.kind == Kind::Req {
        if let Some(docs) = referrers.filter(|d| !d.is_empty()) {
            main.push_str("<h2>この要求を参照している文書</h2>\n<ul>\n");
            for doc in docs {
                let href = encode_href(&relative_path(parent_dir(path), &html_path(doc)), None);
                main.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>\n",
                    esc(&href),
                    esc(&env.pages[doc].title)
                ));
            }
            main.push_str("</ul>\n");
        }
    }
    let nav_html = nav
        .iter()
        .map(|(href, text)| {
            format!(
                "<a href=\"{}\">{}</a>",
                esc(&encode_href(href, None)),
                esc(text)
            )
        })
        .collect::<Vec<_>>()
        .join(" / ");
    html_document(
        &page.title,
        page.description.as_deref(),
        &format!("<nav>{nav_html}</nav>\n"),
        &main,
    )
}

fn html_document(title: &str, description: Option<&str>, nav: &str, main: &str) -> String {
    let mut head = String::new();
    head.push_str("<meta charset=\"utf-8\">\n");
    head.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    head.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self' https:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\">\n");
    head.push_str("<meta name=\"referrer\" content=\"no-referrer\">\n");
    head.push_str(&format!("<title>{}</title>\n", esc(title)));
    if let Some(d) = description {
        head.push_str(&format!(
            "<meta name=\"description\" content=\"{}\">\n",
            esc(d)
        ));
    }
    head.push_str(&format!("<style>{STYLE}</style>\n"));
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n{head}</head>\n<body>\n{nav}<main>\n{main}</main>\n</body>\n</html>\n"
    )
}

fn top_page(
    roots: &[String],
    pages: &BTreeMap<String, Page>,
    title: &str,
    description: Option<&str>,
) -> String {
    let mut main = format!("<h1>{}</h1>\n", esc(title));
    if let Some(d) = description {
        main.push_str(&format!("<p class=\"description\">{}</p>\n", esc(d)));
    }
    main.push_str("<ul>\n");
    for root in roots {
        let index = join_under(root, "index.md");
        match pages.get(&index).filter(|p| p.kind == Kind::Index) {
            Some(page) => {
                main.push_str(&format!(
                    "<li><a href=\"{}\">{}</a>",
                    esc(&encode_href(&html_path(&index), None)),
                    esc(&page.title)
                ));
                if let Some(d) = &page.description {
                    main.push_str(&format!(" — {}", esc(d)));
                }
                main.push_str("</li>\n");
            }
            None => {
                let name = root.rsplit('/').next().unwrap_or(root);
                main.push_str(&format!("<li>{}</li>\n", esc(name)));
            }
        }
    }
    main.push_str("</ul>\n");
    html_document(title, description, "", &main)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(source_dir: &str, dest: &str) -> Option<(String, Option<String>)> {
        resolve_link_path(source_dir, dest).map(|r| (r.path, r.fragment))
    }

    // [FR-106]
    #[test]
    fn 見出しidは小文字化し記号を除き空白をハイフンにする() {
        assert_eq!(heading_slug("Hello, World!"), "hello-world");
        assert_eq!(heading_slug("A_b-c 1"), "a_b-c-1");
    }

    // [FR-106]
    #[test]
    fn 見出しidは連続する空白を1つずつハイフンにする() {
        assert_eq!(heading_slug("a  b"), "a--b");
    }

    // [FR-106]
    #[test]
    fn 見出しidはunicodeの文字と数字を残し小文字にする() {
        assert_eq!(heading_slug("ÄÖ Straße"), "äö-straße");
        assert_eq!(heading_slug("日本語 見出し２"), "日本語-見出し２");
    }

    // [FR-106]
    #[test]
    fn 見出しidはタブと全角空白を除去する() {
        assert_eq!(heading_slug("a\tb"), "ab");
        assert_eq!(heading_slug("a\u{3000}b"), "ab");
        assert_eq!(heading_slug("a b"), "a-b");
    }

    // [FR-106]
    #[test]
    fn 見出しidは記号だけなら空になる() {
        assert_eq!(heading_slug("!!!"), "");
        assert_eq!(heading_slug("***"), "");
    }

    // [FR-107]
    #[test]
    fn リンク解決は元ファイルのディレクトリ基準でドットを正規化する() {
        assert_eq!(
            resolved("docs/a", "../req/x.md"),
            Some(("docs/req/x.md".to_string(), None))
        );
        assert_eq!(
            resolved("docs", "./sub/./y.md"),
            Some(("docs/sub/y.md".to_string(), None))
        );
        assert_eq!(resolved("", "x.md"), Some(("x.md".to_string(), None)));
    }

    // [FR-107]
    #[test]
    fn リンク解決はクエリを捨ててフラグメントを保持する() {
        assert_eq!(
            resolved("docs", "x.md?v=1#top"),
            Some(("docs/x.md".to_string(), Some("top".to_string())))
        );
        assert_eq!(
            resolved("docs", "x.md#top"),
            Some(("docs/x.md".to_string(), Some("top".to_string())))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決はパーセントデコードしデコード後のシャープを再分割しない() {
        assert_eq!(
            resolved("docs", "a%20b.md"),
            Some(("docs/a b.md".to_string(), None))
        );
        assert_eq!(
            resolved("docs", "a%23b.md#f"),
            Some(("docs/a#b.md".to_string(), Some("f".to_string())))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決はクエリだけや空やリポジトリ外をnoneにする() {
        assert_eq!(resolved("docs", ""), None);
        assert_eq!(resolved("docs", "?q=1"), None);
        assert_eq!(resolved("docs", "../../x.md"), None);
    }

    // [FR-107]
    #[test]
    fn リンク解決はクエリを先に捨ててからデコードする() {
        assert_eq!(
            resolved("docs", "a%3Fb.md"),
            Some(("docs/a?b.md".to_string(), None))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決はフラグメント中のクエスチョンを保持する() {
        assert_eq!(
            resolved("docs", "x.md#a?b"),
            Some(("docs/x.md".to_string(), Some("a?b".to_string())))
        );
    }

    // [FR-107]
    #[test]
    fn リンク解決のドットは自ディレクトリを指す() {
        assert_eq!(
            resolved("docs/sub", "."),
            Some(("docs/sub".to_string(), None))
        );
    }

    // [FR-107]
    #[test]
    fn hrefはpchar以外をセグメントごとにパーセントエンコードする() {
        assert_eq!(encode_href("docs/a b.html", None), "docs/a%20b.html");
        assert_eq!(
            encode_href("docs/日本.html", None),
            "docs/%E6%97%A5%E6%9C%AC.html"
        );
        assert_eq!(encode_href("docs/a#b.html", None), "docs/a%23b.html");
        assert_eq!(encode_href("docs/a?b.html", None), "docs/a%3Fb.html");
        assert_eq!(encode_href("docs/a@b+c.html", None), "docs/a@b+c.html");
        assert_eq!(encode_href("docs/100%.html", None), "docs/100%25.html");
    }

    // [FR-107]
    #[test]
    fn hrefの最初のセグメントのコロンだけを3aにする() {
        assert_eq!(encode_href("a:b/c.html", None), "a%3Ab/c.html");
        assert_eq!(encode_href("docs/a:b.html", None), "docs/a:b.html");
    }

    // [FR-107]
    #[test]
    fn hrefはフラグメントの有効なパーセント表記を保持し他のパーセントを符号化する() {
        assert_eq!(encode_href("x.html", Some("%E6%A6%82")), "x.html#%E6%A6%82");
        assert_eq!(encode_href("x.html", Some("100%")), "x.html#100%25");
    }

    // [FR-107]
    #[test]
    fn hrefはフラグメントをシャープで付ける() {
        assert_eq!(encode_href("x.html", Some("sec")), "x.html#sec");
    }
}
