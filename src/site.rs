//! `miwa site`: static HTML output for reading the documents.
//!
//! The site mirrors the index tree: every published page is a document body
//! (or an index page) rendered from its Markdown source, with navigation built
//! from the index structure (FR-089 .. FR-107).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

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
enum Kind {
    /// A req node (`type: Requirement`); lists the docs referencing it.
    Req,
    /// Any other document (including doc nodes). Whether an id is shown is
    /// decided by `Page::id`, not by the kind.
    Plain,
    /// An `index.md` under an index root.
    Index,
}

struct Page {
    kind: Kind,
    id: Option<String>,
    title: String,
    description: Option<String>,
    body: String,
}

/// What rendering one page produced besides its HTML.
#[derive(Default)]
struct Rendered {
    warnings: Vec<String>,
    images: BTreeSet<String>,
}

struct Env<'a> {
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
    let out_dir = repo_root.join(out);
    check_out_dir(&out_dir)?;

    let source = FsSource::new(repo_root);
    let graph = crate::graph::build_graph(&source, config)?;
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

    let mut warnings: Vec<String> = Vec::new();
    for root in &roots {
        let root_index = join_under(root, "index.md");
        if !matches!(pages.get(&root_index), Some(p) if p.kind == Kind::Index) {
            warnings.push(format!("warning: {root_index}: missing index"));
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

    let file_set: HashSet<String> = files.iter().cloned().collect();
    let media_roots: Vec<String> = config
        .docs
        .iter()
        .chain(config.requirements.iter())
        .map(|d| normalize_dir(d))
        .collect();
    let env = Env {
        pages: &pages,
        files: &file_set,
        media_roots: &media_roots,
    };

    let (output, images) = render_pages(
        &env,
        &trees,
        &referrers,
        &roots,
        &site_title,
        site_description.as_deref(),
        &mut warnings,
    );
    write_output(&out_dir, &source, &output, &images)?;
    Ok(warnings)
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
            let body = body_of(&texts[&d.path].text);
            pages.insert(
                d.path,
                Page {
                    kind: Kind::Plain,
                    id: None,
                    title: d.title,
                    description: d.description,
                    body,
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
                },
            );
        }
        trees.push((root.clone(), IndexTree::new(root, index_paths)));
    }
    // A graph node overrides a collect_documents entry for the same path.
    for node in graph.nodes.values() {
        let kind = match node.key.kind {
            NodeKind::Doc => Kind::Plain,
            NodeKind::Req => Kind::Req,
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
            },
        );
    }
    (pages, trees)
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

fn esc(s: &str) -> String {
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

/// The URI scheme of `dest` (without the colon), if it starts with one.
impl Env<'_> {
    /// The output page (repository-relative `.html` path) a resolved path
    /// publishes as: a published `.md` page, or the published `index.md` of a
    /// directory.
    fn target_page(&self, path: &str) -> Option<String> {
        if path.ends_with(".md") && self.pages.contains_key(path) {
            return Some(html_path(path));
        }
        let index = format!("{path}/index.md");
        match self.pages.get(&index) {
            Some(p) if p.kind == Kind::Index => Some(html_path(&index)),
            _ => None,
        }
    }

    /// The href for a link destination, or `None` when it is not published
    /// (FR-098, FR-099).
    fn link_href(&self, src_path: &str, dest: &str) -> Option<String> {
        if let Some(rest) = dest.strip_prefix('#') {
            return Some(format!("#{}", encode_fragment(rest)));
        }
        if let Some(scheme) = uri_scheme(dest) {
            let ok = ["http", "https", "mailto"]
                .iter()
                .any(|s| scheme.eq_ignore_ascii_case(s));
            return ok.then(|| dest.to_string());
        }
        if dest.starts_with('/') {
            return None;
        }
        let src_dir = parent_dir(src_path);
        let resolved = resolve_link_path(src_dir, dest)?;
        let target = self.target_page(&resolved.path)?;
        Some(encode_href(
            &relative_path(src_dir, &target),
            resolved.fragment.as_deref(),
        ))
    }

    /// The `src` for an image destination, or `None` when it is not published
    /// (FR-100). Copied images are recorded in `images`.
    fn image_src(
        &self,
        src_path: &str,
        dest: &str,
        images: &mut BTreeSet<String>,
    ) -> Option<String> {
        if let Some(scheme) = uri_scheme(dest) {
            return scheme
                .eq_ignore_ascii_case("https")
                .then(|| dest.to_string());
        }
        if dest.starts_with('/') {
            return None;
        }
        let src_dir = parent_dir(src_path);
        let resolved = resolve_link_path(src_dir, dest)?;
        let ext = resolved.path.rsplit_once('.')?.1.to_ascii_lowercase();
        if !["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
            return None;
        }
        if !self.files.contains(&resolved.path)
            || !self.media_roots.iter().any(|r| is_under(&resolved.path, r))
        {
            return None;
        }
        let href = encode_href(
            &relative_path(src_dir, &resolved.path),
            resolved.fragment.as_deref(),
        );
        images.insert(resolved.path);
        Some(href)
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

/// Converts the Markdown `body` of the page at `src_path` to HTML. When the
/// first block is a level-1 heading, `meta` (id and summary) is placed right
/// after it and the second value is `true`.
/// Whether the first block is a non-empty level-1 heading. HTML comment
/// blocks (and inline comments) are not blocks and are skipped.
fn starts_with_h1(events: &[Event]) -> bool {
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::HtmlBlock) => {
                let mut text = String::new();
                i += 1;
                while i < events.len() {
                    match &events[i] {
                        Event::End(TagEnd::HtmlBlock) => break,
                        Event::Html(s) | Event::Text(s) => text.push_str(s),
                        _ => {}
                    }
                    i += 1;
                }
                if !is_comment_html(&text) {
                    return false;
                }
            }
            Event::InlineHtml(s) if is_comment_html(s) => {}
            Event::Start(Tag::Heading { level, .. }) => {
                let empty = matches!(events.get(i + 1), Some(Event::End(TagEnd::Heading(_))));
                return *level == HeadingLevel::H1 && !empty;
            }
            _ => return false,
        }
        i += 1;
    }
    false
}

fn render_body(
    env: &Env,
    src_path: &str,
    body: &str,
    meta: &str,
    rendered: &mut Rendered,
) -> (String, bool) {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let events: Vec<Event> = Parser::new_ext(body, options).collect();

    let mut used_ids: HashSet<String> = HashSet::new();
    // Next suffix to try per slug (avoids rescanning `slug-1`, `slug-2`, ...).
    let mut next_suffix: HashMap<String, usize> = HashMap::new();
    for ev in &events {
        match ev {
            Event::FootnoteReference(label) | Event::Start(Tag::FootnoteDefinition(label)) => {
                used_ids.insert(label.to_string());
            }
            _ => {}
        }
    }

    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    // Whether each open link / image is kept (an unpublished one loses its tags).
    let mut kept: Vec<bool> = Vec::new();
    let mut depth = 0usize;
    let first_is_h1 = starts_with_h1(&events);
    let mut injected = false;
    let mut i = 0;
    while i < events.len() {
        let ev = events[i].clone();
        i += 1;
        match ev {
            Event::Start(Tag::HtmlBlock) => {
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
                if is_comment_html(&text) {
                    continue;
                }
                out.push(Event::Html(
                    format!("<pre>{}</pre>\n", esc(&strip_comments(&text))).into(),
                ));
            }
            Event::InlineHtml(s) => {
                if !is_comment_html(&s) {
                    out.push(Event::Text(s));
                }
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
                    Some(CowStr::from(candidate))
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
                if depth == 0 && first_is_h1 && !injected {
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
                match env.link_href(src_path, &dest) {
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
                    None => {
                        kept.push(false);
                        rendered.warnings.push(format!(
                            "warning: {src_path}: unpublished link: {}",
                            escape_controls(&dest_url)
                        ));
                    }
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
                match env.image_src(src_path, &dest_url, &mut rendered.images) {
                    Some(src) => {
                        kept.push(true);
                        out.push(Event::Start(Tag::Image {
                            link_type,
                            dest_url: src.into(),
                            title,
                            id,
                        }));
                    }
                    None => {
                        kept.push(false);
                        rendered.warnings.push(format!(
                            "warning: {src_path}: unpublished image: {}",
                            escape_controls(&dest_url)
                        ));
                    }
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
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    (html, first_is_h1)
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
