//! Index (`index.md`) structure checks and the `miwa index` command.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::frontmatter::{FrontMatter, FrontMatterError};
use crate::model::ReasonCode;

/// One list entry of an index: a list item that starts with a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Source text of the link text (between `[` and the matching `]`).
    pub link_text: String,
    /// Link destination as interpreted by pulldown-cmark (without `<...>` and
    /// the title attribute). Used for comparison and resolution only; never
    /// written back.
    pub dest: String,
    /// Summary with the separator and surrounding whitespace removed.
    pub description: String,
    /// One of `-` `–` `—` `:`, if present.
    pub separator: Option<char>,
    /// From just after `[` to just before the matching `]`.
    pub text_span: Range<usize>,
    /// From the end of the link to the end of the inline content (separator +
    /// summary, may span lines).
    pub tail_span: Range<usize>,
    /// From the start of the line containing `text_span.start` through the line
    /// terminator (`\n` or `\r\n`) after `tail_span.end`.
    pub line_span: Range<usize>,
}

/// How an entry's link destination resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// External URL / URI scheme: ignored.
    External,
    /// Cannot be resolved (empty, absolute, escapes repo, outside root, self or
    /// ancestor directory); the reason is used as the finding detail.
    Unresolvable(&'static str),
    /// Repo-relative file path (may not exist).
    File(String),
    /// Repo-relative directory whose `index.md` is the target.
    Directory(String),
}

/// A typed document (has frontmatter `type`) under an index root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
}

/// An index structure problem. `target` follows the canonical form: for
/// `BrokenEntry` / `MissingDirectoryDescription` it is
/// `"<index path> -> <dest>"`, otherwise the path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexIssue {
    pub reason: ReasonCode,
    pub target: String,
    pub detail: String,
}

/// File body and parsed frontmatter, read once by `build_graph`.
#[derive(Debug)]
pub struct ParsedText {
    pub text: String,
    pub front_matter: Result<Option<FrontMatter>, FrontMatterError>,
}

const BOM: char = '\u{feff}';

fn unescape_brackets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && matches!(chars.peek(), Some('\\' | '[' | ']')) {
            out.push(chars.next().unwrap_or(c));
        } else {
            out.push(c);
        }
    }
    out
}

fn escape_brackets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' | '(' | ')' | '<' | '>' | '%' | '#' | '\\' => {
                out.push_str(&format!("%{:02X}", c as u32));
            }
            _ => out.push(c),
        }
    }
    out
}

fn parent_dir(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

fn is_inline_event(ev: &Event) -> bool {
    match ev {
        Event::Text(_)
        | Event::Code(_)
        | Event::InlineMath(_)
        | Event::DisplayMath(_)
        | Event::InlineHtml(_)
        | Event::FootnoteReference(_)
        | Event::SoftBreak
        | Event::HardBreak => true,
        Event::Start(t) => matches!(
            t,
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
        ),
        Event::End(t) => matches!(
            t,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        ),
        _ => false,
    }
}

/// Extract index entries from `text`.
///
/// Parses with `Parser::new_ext(text, ENABLE_TASKLISTS).into_offset_iter()`. A
/// list item (any nesting, ordered or unordered) is an entry iff its first
/// inline event (skipping `Start(Paragraph)`) is `Start(Link)`. Task-list
/// markers, bold, plain text etc. are non-entries. A leading BOM is ignored for
/// parsing, and spans are offsets into `text` including the BOM. `text_span` covers all ranged events inside the
/// link (empty link: `start+1..start+1`); `tail_span` covers inline events after
/// `End(Link)` up to `End(Item)` / `End(Paragraph)` / `Start(List)` (excluding
/// soft/hard breaks). `description` is `tail_span` trimmed, minus one leading
/// separator char, trim_start; `\[` `\]` are unescaped for comparison.
pub fn parse_entries(text: &str) -> Vec<IndexEntry> {
    let (body, off) = match text.strip_prefix(BOM) {
        Some(b) => (b, BOM.len_utf8()),
        None => (text, 0),
    };
    let events: Vec<(Event, Range<usize>)> = Parser::new_ext(body, Options::ENABLE_TASKLISTS)
        .into_offset_iter()
        .collect();
    let mut entries = Vec::new();
    for i in 0..events.len() {
        if !matches!(events[i].0, Event::Start(Tag::Item)) {
            continue;
        }
        let mut k = i + 1;
        if matches!(events.get(k), Some((Event::Start(Tag::Paragraph), _))) {
            k += 1;
        }
        let (dest_url, link_type, link_range) = match events.get(k) {
            Some((
                Event::Start(Tag::Link {
                    dest_url,
                    link_type,
                    ..
                }),
                r,
            )) => (dest_url.to_string(), *link_type, r.clone()),
            _ => continue,
        };
        let mut j = k + 1;
        let mut t_max = link_range.start + 1;
        while j < events.len() && !matches!(events[j].0, Event::End(TagEnd::Link)) {
            t_max = t_max.max(events[j].1.end);
            j += 1;
        }
        // pulldown-cmark's Text ranges omit the backslash of an escape, so the
        // start is pinned to just after `[`, and the end is the first `]` at or
        // after the end of the last inner event.
        let close = body.as_bytes()[t_max.min(body.len())..]
            .iter()
            .position(|&b| b == b']')
            .map_or(t_max, |p| t_max + p);
        let text_span = link_range.start + 1 + off..close + off;
        // The End(Link) range of a collapsed reference `[T][]` stops before `[]`.
        let mut link_end = link_range.end;
        if link_type == LinkType::Collapsed && body[link_end..].starts_with("[]") {
            link_end += 2;
        }
        let mut m = j + 1;
        let mut a_max = None::<usize>;
        while m < events.len() && is_inline_event(&events[m].0) {
            if !matches!(events[m].0, Event::SoftBreak | Event::HardBreak) {
                a_max = Some(a_max.map_or(events[m].1.end, |x| x.max(events[m].1.end)));
            }
            m += 1;
        }
        let tail_end = a_max.map_or(link_end, |x| x.max(link_end));
        let tail_span = link_end + off..tail_end + off;
        let raw = &text[tail_span.clone()];
        let trimmed = raw.trim();
        let (separator, description) = match trimmed.chars().next() {
            Some(c) if matches!(c, '-' | '–' | '—' | ':') => {
                (Some(c), trimmed[c.len_utf8()..].trim_start().to_string())
            }
            _ => (None, trimmed.to_string()),
        };
        let line_start = text[..text_span.start]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0)
            .max(off);
        let line_end = match text[tail_span.end..].find('\n') {
            Some(p) => tail_span.end + p + 1,
            None => text.len(),
        };
        entries.push(IndexEntry {
            link_text: text[text_span.clone()].to_string(),
            dest: dest_url,
            description,
            separator,
            text_span,
            tail_span,
            line_span: line_start..line_end,
        });
    }
    entries
}

/// Resolve an entry destination relative to `index_path` within `root`.
///
/// Order: empty / `#...` -> `Unresolvable("empty target")`; `://` or URI scheme
/// -> `External`; leading `/` -> `Unresolvable("absolute path")`; strip
/// fragment, percent-decode, `markdown::normalize_relative` (escape ->
/// `Unresolvable("escapes repository")`); outside `root` ->
/// `Unresolvable("outside index root")`; trailing `/` or file name `index.md`
/// -> directory candidate (`index.md` -> its parent), and a candidate that is
/// the index's own directory or an ancestor -> `Unresolvable("self or ancestor
/// directory")`, else `Directory`; otherwise `File` if present in `file_set`,
/// or `Directory` if some file lies under `path/` (`repo.dirs`), else `File`
/// (missing).
pub fn resolve(index_path: &str, root: &str, dest: &str, repo: &RepoFiles) -> Resolution {
    if dest.is_empty() || dest.starts_with('#') {
        return Resolution::Unresolvable("empty target");
    }
    if dest.contains("://") || crate::markdown::has_uri_scheme(dest) {
        return Resolution::External;
    }
    if dest.starts_with('/') {
        return Resolution::Unresolvable("absolute path");
    }
    let no_frag = dest.split('#').next().unwrap_or("");
    let decoded = crate::markdown::percent_decode(no_frag);
    let path = match crate::markdown::normalize_relative(index_path, &decoded) {
        Some(p) => p,
        None => return Resolution::Unresolvable("escapes repository"),
    };
    if !crate::config::is_under(&path, root) {
        return Resolution::Unresolvable("outside index root");
    }
    let own_dir = parent_dir(index_path);
    let dir_candidate = |dir: String| {
        if own_dir == dir || own_dir.starts_with(&format!("{dir}/")) {
            Resolution::Unresolvable("self or ancestor directory")
        } else {
            Resolution::Directory(dir)
        }
    };
    let file_name = path.rsplit('/').next().unwrap_or("");
    if decoded.ends_with('/') {
        return dir_candidate(path);
    }
    if file_name == "index.md" {
        return dir_candidate(parent_dir(&path).to_string());
    }
    if repo.files.contains(path.as_str()) {
        return Resolution::File(path);
    }
    if repo.dirs.contains(path.as_str()) {
        return dir_candidate(path);
    }
    Resolution::File(path)
}

/// Repo files plus every ancestor directory of them, for O(1) lookups.
pub struct RepoFiles<'a> {
    pub files: HashSet<&'a str>,
    pub dirs: HashSet<String>,
}

impl<'a> RepoFiles<'a> {
    pub fn new(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let files: HashSet<&str> = paths.into_iter().collect();
        let mut dirs = HashSet::new();
        for f in &files {
            let mut cur = parent_dir(f);
            while !cur.is_empty() && dirs.insert(cur.to_string()) {
                cur = parent_dir(cur);
            }
        }
        RepoFiles { files, dirs }
    }
}

/// Ownership and parent/child relations among the indexes of one root.
pub struct IndexTree {
    root: String,
    indexes: BTreeSet<String>,
}

impl IndexTree {
    pub fn new(root: &str, indexes: impl IntoIterator<Item = String>) -> Self {
        IndexTree {
            root: root.to_string(),
            indexes: indexes.into_iter().collect(),
        }
    }

    fn root_index(&self) -> String {
        format!("{}/index.md", self.root)
    }

    /// Nearest index at or above `dir`, never above the root (the root index
    /// even when it does not exist yet).
    fn nearest(&self, dir: &str) -> String {
        let mut cur = dir;
        loop {
            let cand = format!("{cur}/index.md");
            if self.indexes.contains(&cand) {
                return cand;
            }
            if cur == self.root || cur.is_empty() || !crate::config::is_under(cur, &self.root) {
                return self.root_index();
            }
            cur = parent_dir(cur);
        }
    }

    /// The index responsible for listing the document at `doc_path`.
    pub fn owner_of(&self, doc_path: &str) -> String {
        self.nearest(parent_dir(doc_path))
    }

    /// The index that should link to `index_path`; `None` for the root index.
    pub fn parent_of(&self, index_path: &str) -> Option<String> {
        let dir = parent_dir(index_path);
        if dir == self.root {
            None
        } else {
            Some(self.nearest(parent_dir(dir)))
        }
    }

    /// Indexes whose parent is `index_path`.
    pub fn children_of(&self, index_path: &str) -> Vec<String> {
        self.indexes
            .iter()
            .filter(|c| self.parent_of(c).as_deref() == Some(index_path))
            .cloned()
            .collect()
    }
}

/// Result of one index processed by [`run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    Created,
    Updated,
    Unchanged,
}

impl IndexStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IndexStatus::Created => "created",
            IndexStatus::Updated => "updated",
            IndexStatus::Unchanged => "unchanged",
        }
    }
}

/// The `.md` files under an index root, classified.
#[derive(Debug, Default)]
pub struct RootFiles {
    pub documents: Vec<Document>,
    pub untyped: Vec<String>,
    pub indexes: Vec<String>,
}

/// Classify the `.md` files under `root` into documents, untyped files and
/// indexes.
///
/// `index.md` -> indexes; `log.md` -> ignored; frontmatter error -> ignored
/// (already a ParseError); no frontmatter or no `type` -> untyped; otherwise a
/// `Document`.
pub fn collect_documents(root: &str, texts: &BTreeMap<String, ParsedText>) -> RootFiles {
    let mut out = RootFiles::default();
    for (path, parsed) in texts {
        if !path.ends_with(".md") || !crate::config::is_under(path, root) {
            continue;
        }
        match path.rsplit('/').next() {
            Some("index.md") => {
                out.indexes.push(path.clone());
                continue;
            }
            Some("log.md") => continue,
            _ => {}
        }
        match &parsed.front_matter {
            Err(_) => {}
            Ok(Some(fm)) if fm.kind.is_some() => {
                let description = fm
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|d| !d.is_empty() && !d.contains('\n'))
                    .map(str::to_string);
                out.documents.push(Document {
                    path: path.clone(),
                    title: document_title(path, fm),
                    description,
                });
            }
            Ok(_) => out.untyped.push(path.clone()),
        }
    }
    out
}

/// Title of a document: frontmatter `title` (trimmed, non-empty, single line)
/// -> first H1 of the body (source text of the inner events) -> file name
/// without `.md`.
pub fn document_title(path: &str, fm: &FrontMatter) -> String {
    if let Some(t) = fm.title.as_deref().map(str::trim) {
        if !t.is_empty() && !t.contains('\n') && !t.contains('\r') {
            return t.to_string();
        }
    }
    let body = fm.body.as_str();
    let mut in_h1 = false;
    let (mut min, mut max) = (usize::MAX, 0usize);
    for (ev, range) in Parser::new(body).into_offset_iter() {
        match ev {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) => {
                in_h1 = true;
                min = usize::MAX;
                max = 0;
            }
            Event::End(TagEnd::Heading(_)) if in_h1 => {
                // Only the first level-1 heading counts; an empty one falls
                // back to the file name.
                if min != usize::MAX {
                    return body[min..max].to_string();
                }
                break;
            }
            _ if in_h1 => {
                min = min.min(range.start);
                max = max.max(range.end);
            }
            _ => {}
        }
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

fn is_excluded(path: &str, config: &crate::config::Config) -> bool {
    crate::source::is_under_dir_named(path, &config.exclude)
        || crate::source::is_under_dir_named(path, &crate::source::DEFAULT_EXCLUDED_DIRS)
}

/// `<index path> -> <dest>`, the finding target of an entry.
fn entry_target(index: &str, dest: &str) -> String {
    format!("{index} -> {dest}")
}

fn issue(reason: ReasonCode, target: impl Into<String>, detail: impl Into<String>) -> IndexIssue {
    IndexIssue {
        reason,
        target: target.into(),
        detail: detail.into(),
    }
}

/// Run all index checks for every root in `config.index`.
///
/// Per root: `UntypedFile` for untyped files and `MissingDescription` for
/// documents without a description; `MissingIndex` (target = root) if there is
/// no `root/index.md` and at least one document (then the index-side checks are
/// skipped); each document / non-root index is owned by the nearest ancestor
/// index; each entry is resolved: `External` ignored, `Unresolvable` ->
/// `BrokenEntry`, `File` not a document -> `BrokenEntry`, title mismatch ->
/// `TitleMismatch`, description mismatch -> `DescriptionMismatch`; `Directory`
/// without `index.md` -> `BrokenEntry`, empty summary ->
/// `MissingDirectoryDescription`; unlisted documents -> `NotListed`; child
/// indexes not reached from the parent -> `UnlinkedIndex`. Duplicates are
/// removed by the set.
pub fn collect_issues(
    config: &crate::config::Config,
    files: &[String],
    texts: &BTreeMap<String, ParsedText>,
) -> BTreeSet<IndexIssue> {
    let mut out = BTreeSet::new();
    let repo = RepoFiles::new(files.iter().map(String::as_str));
    for raw in &config.index {
        let root = crate::config::normalize_dir(raw);
        check_root(config, &root, &repo, texts, &mut out);
    }
    out
}

fn check_root(
    config: &crate::config::Config,
    root: &str,
    repo: &RepoFiles,
    texts: &BTreeMap<String, ParsedText>,
    out: &mut BTreeSet<IndexIssue>,
) {
    let RootFiles {
        documents,
        untyped,
        indexes,
    } = collect_documents(root, texts);
    for u in &untyped {
        out.insert(issue(
            ReasonCode::UntypedFile,
            u.clone(),
            "file has no `type` in its front matter",
        ));
    }
    for d in &documents {
        if d.description.is_none() {
            out.insert(issue(
                ReasonCode::MissingDescription,
                d.path.clone(),
                "document has no single-line `description`",
            ));
        }
    }
    let root_index = crate::config::join_under(root, "index.md");
    let index_set: HashSet<&str> = indexes.iter().map(String::as_str).collect();
    if !index_set.contains(root_index.as_str()) {
        if !documents.is_empty() {
            out.insert(issue(
                ReasonCode::MissingIndex,
                root.to_string(),
                format!("{root_index} does not exist"),
            ));
        }
        return;
    }
    let doc_map: HashMap<&str, &Document> =
        documents.iter().map(|d| (d.path.as_str(), d)).collect();
    let untyped_set: HashSet<&str> = untyped.iter().map(String::as_str).collect();
    let tree = IndexTree::new(root, indexes.iter().cloned());
    let mut listed: HashMap<&str, HashSet<String>> = HashMap::new();
    let mut linked: HashSet<String> = HashSet::new();
    for index in &indexes {
        let text = &texts[index].text;
        for e in parse_entries(text) {
            match resolve(index, root, &e.dest, repo) {
                Resolution::External => {}
                Resolution::Unresolvable(why) => {
                    out.insert(issue(
                        ReasonCode::BrokenEntry,
                        entry_target(index, &e.dest),
                        why,
                    ));
                }
                Resolution::File(p) => match doc_map.get(p.as_str()) {
                    Some(doc) => {
                        listed.entry(index).or_default().insert(p.clone());
                        if unescape_brackets(&e.link_text) != doc.title {
                            out.insert(issue(
                                ReasonCode::TitleMismatch,
                                p.clone(),
                                format!(
                                    "link text differs from the document title `{}`",
                                    doc.title
                                ),
                            ));
                        }
                        if let Some(d) = &doc.description {
                            if unescape_brackets(&e.description) != *d {
                                out.insert(issue(
                                    ReasonCode::DescriptionMismatch,
                                    p.clone(),
                                    format!("summary differs from the document description `{d}`"),
                                ));
                            }
                        }
                    }
                    None => {
                        let why = if is_excluded(&p, config) {
                            "target is excluded from scan"
                        } else if !repo.files.contains(p.as_str()) {
                            "target does not exist"
                        } else if untyped_set.contains(p.as_str()) {
                            "target has no `type`"
                        } else {
                            "target is not a document"
                        };
                        out.insert(issue(
                            ReasonCode::BrokenEntry,
                            entry_target(index, &e.dest),
                            why,
                        ));
                    }
                },
                Resolution::Directory(d) => {
                    let child = format!("{d}/index.md");
                    if !index_set.contains(child.as_str()) {
                        out.insert(issue(
                            ReasonCode::BrokenEntry,
                            entry_target(index, &e.dest),
                            "directory has no index.md",
                        ));
                    } else {
                        if e.description.is_empty() {
                            out.insert(issue(
                                ReasonCode::MissingDirectoryDescription,
                                entry_target(index, &e.dest),
                                "directory entry has no summary",
                            ));
                        }
                        if tree.parent_of(&child).as_deref() == Some(index.as_str()) {
                            linked.insert(child);
                        }
                    }
                }
            }
        }
    }
    for d in &documents {
        let owner = tree.owner_of(&d.path);
        let is_listed = listed
            .get(owner.as_str())
            .is_some_and(|s| s.contains(&d.path));
        if !is_listed {
            out.insert(issue(
                ReasonCode::NotListed,
                d.path.clone(),
                format!("not listed in {owner}"),
            ));
        }
    }
    for index in &indexes {
        if *index != root_index && !linked.contains(index) {
            out.insert(issue(
                ReasonCode::UnlinkedIndex,
                index.clone(),
                "not linked from its parent index",
            ));
        }
    }
}

fn is_list_item_line(line: &str) -> bool {
    let l = line.trim_start();
    if l.starts_with("- ") || l.starts_with("* ") || l.starts_with("+ ") {
        return true;
    }
    let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && {
        let rest = &l[digits..];
        rest.starts_with(". ") || rest.starts_with(") ")
    }
}

/// Return the updated text of an index.
///
/// Only `text_span` and `tail_span` are rewritten; link destinations are never
/// touched. `documents` are all documents of the root; ownership is derived
/// from `tree`. For `File(p)` entries that are
/// documents: `text_span` becomes the escaped title (`[` `]` -> `\[` `\]`;
/// not for collapsed / shortcut references, whose link text doubles as the
/// reference label), `tail_span` becomes ` {sep} {desc}` (sep = existing or
/// `—`; empty if no description). Entries whose `File(p)` does not exist
/// (`exists` false) and is not under a default-excluded directory have their
/// `line_span` removed. Then missing owned documents (path order) and unlinked
/// child indexes (path order) are appended as list items, with percent-encoded
/// destinations, matching the text's line ending, and a blank line inserted
/// when the last non-empty line is not a list item.
pub fn update_index(
    text: &str,
    index_path: &str,
    tree: &IndexTree,
    documents: &[Document],
    repo: &RepoFiles,
    unlinked_child_indexes: &[String],
    excluded: &[String],
) -> String {
    let root = tree.root.as_str();
    let index_dir = parent_dir(index_path);
    let doc_map: HashMap<&str, &Document> =
        documents.iter().map(|d| (d.path.as_str(), d)).collect();
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut listed: HashSet<String> = HashSet::new();
    for e in parse_entries(text) {
        let p = match resolve(index_path, root, &e.dest, repo) {
            Resolution::File(p) => p,
            _ => continue,
        };
        if let Some(doc) = doc_map.get(p.as_str()) {
            listed.insert(p);
            let after = &text.as_bytes()[e.text_span.end..];
            // Collapsed `[T][]` and shortcut `[T]` references use the link text as
            // the reference label, so it must not change.
            let keep_label = after.first() == Some(&b']')
                && match after.get(1) {
                    Some(b'(') => false,
                    Some(b'[') => after.get(2) == Some(&b']'),
                    _ => true,
                };
            if !keep_label {
                edits.push((e.text_span.clone(), escape_brackets(&doc.title)));
            }
            let tail = match &doc.description {
                Some(d) => format!(" {} {}", e.separator.unwrap_or('—'), escape_brackets(d)),
                None => String::new(),
            };
            edits.push((e.tail_span.clone(), tail));
        } else if !repo.files.contains(p.as_str())
            && !crate::source::is_under_dir_named(&p, excluded)
        {
            edits.push((e.line_span.clone(), String::new()));
        }
    }
    edits.sort_by_key(|e| e.0.start);
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for (range, rep) in edits {
        out.push_str(&text[pos..range.start]);
        out.push_str(&rep);
        pos = range.end;
    }
    out.push_str(&text[pos..]);

    let rel = |path: &str| -> String {
        path.strip_prefix(&format!("{index_dir}/"))
            .unwrap_or(path)
            .to_string()
    };
    let mut missing: Vec<&Document> = documents
        .iter()
        .filter(|d| tree.owner_of(&d.path) == index_path && !listed.contains(&d.path))
        .collect();
    missing.sort_by(|a, b| a.path.cmp(&b.path));
    let mut children: Vec<&String> = unlinked_child_indexes.iter().collect();
    children.sort();
    let mut lines: Vec<String> = Vec::new();
    for d in missing {
        let mut l = format!(
            "- [{}]({})",
            escape_brackets(&d.title),
            percent_encode(&rel(&d.path))
        );
        if let Some(desc) = &d.description {
            l.push_str(&format!(" — {}", escape_brackets(desc)));
        }
        lines.push(l);
    }
    for c in children {
        let dir = rel(parent_dir(c));
        lines.push(format!(
            "- [{}]({}/) — ",
            escape_brackets(&dir),
            percent_encode(&dir)
        ));
    }
    if lines.is_empty() {
        return out;
    }
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(nl);
        }
        if let Some(last) = out.lines().rev().find(|l| !l.trim().is_empty()) {
            if !is_list_item_line(last) {
                out.push_str(nl);
            }
        }
    }
    for l in lines {
        out.push_str(&l);
        out.push_str(nl);
    }
    out
}

fn write_atomic(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!(
        ".index.md.{}.{nanos}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let tmp = path.with_file_name(name);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let tmp_ref = &tmp;
    let result = (move || -> std::io::Result<()> {
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(tmp_ref, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// `miwa index`: create or update the indexes of every root in `config.index`.
///
/// Errors if `config.index` is empty ("index is not configured in
/// .miwa/config.yaml"). Creates a missing root index as a flat list, otherwise
/// updates the root index and its descendants. Each file is written to a
/// temporary file in the same directory and renamed into place. Returns the
/// status of every processed index, in processing order; nothing is printed.
pub fn run(
    repo_root: &std::path::Path,
    config: &crate::config::Config,
) -> anyhow::Result<Vec<(IndexStatus, String)>> {
    use crate::source::Source;
    if config.index.is_empty() {
        anyhow::bail!("index is not configured in .miwa/config.yaml");
    }
    let source = crate::source::FsSource::new(repo_root);
    let files: Vec<String> = source
        .list_files()?
        .into_iter()
        .filter(|p| !crate::source::is_under_dir_named(p, &config.exclude))
        .collect();
    let repo = RepoFiles::new(files.iter().map(String::as_str));
    let roots: Vec<String> = config
        .index
        .iter()
        .map(|r| crate::config::normalize_dir(r))
        .collect();
    let mut texts: BTreeMap<String, ParsedText> = BTreeMap::new();
    for path in &files {
        if path.ends_with(".md") && roots.iter().any(|r| crate::config::is_under(path, r)) {
            let bytes = source.read(path)?;
            let text = if path.rsplit('/').next() == Some("index.md") {
                String::from_utf8(bytes).map_err(|_| {
                    anyhow::anyhow!("{path} is not valid UTF-8; refusing to rewrite it")
                })?
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            let front_matter = crate::frontmatter::parse(&text);
            texts.insert(path.clone(), ParsedText { text, front_matter });
        }
    }
    let excluded: Vec<String> = config
        .exclude
        .iter()
        .cloned()
        .chain(
            crate::source::DEFAULT_EXCLUDED_DIRS
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    let mut results = Vec::new();
    for root in &roots {
        let RootFiles {
            documents, indexes, ..
        } = collect_documents(root, &texts);
        let tree = IndexTree::new(root, indexes.iter().cloned());
        let root_index = crate::config::join_under(root, "index.md");
        let targets: Vec<String> = if indexes.contains(&root_index) {
            indexes.clone()
        } else if documents.is_empty() {
            continue;
        } else {
            let mut all = vec![root_index.clone()];
            all.extend(indexes.iter().cloned());
            all
        };
        for index in targets {
            let existing = texts.get(&index).map(|t| t.text.as_str());
            let text = existing.unwrap_or("");
            let linked: HashSet<String> = parse_entries(text)
                .into_iter()
                .filter_map(|e| match resolve(&index, root, &e.dest, &repo) {
                    Resolution::Directory(d) => Some(format!("{d}/index.md")),
                    _ => None,
                })
                .collect();
            let unlinked: Vec<String> = tree
                .children_of(&index)
                .into_iter()
                .filter(|c| !linked.contains(c))
                .collect();
            let new_text =
                update_index(text, &index, &tree, &documents, &repo, &unlinked, &excluded);
            let status = if existing.is_none() {
                IndexStatus::Created
            } else if new_text == text {
                IndexStatus::Unchanged
            } else {
                IndexStatus::Updated
            };
            if status != IndexStatus::Unchanged {
                write_atomic(&repo_root.join(&index), &new_text)?;
            }
            results.push((status, index));
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    // ---- helpers -------------------------------------------------------

    fn dests(text: &str) -> Vec<String> {
        parse_entries(text).into_iter().map(|e| e.dest).collect()
    }

    fn doc(path: &str, title: &str, description: Option<&str>) -> Document {
        Document {
            path: path.to_string(),
            title: title.to_string(),
            description: description.map(str::to_string),
        }
    }

    /// A typed document source with `title` / `description` front matter.
    fn typed(title: &str, description: Option<&str>) -> String {
        match description {
            Some(d) => format!("---\ntype: Specification\ntitle: {title}\ndescription: {d}\n---\n"),
            None => format!("---\ntype: Specification\ntitle: {title}\n---\n"),
        }
    }

    fn texts_of(files: &[(&str, &str)]) -> BTreeMap<String, ParsedText> {
        files
            .iter()
            .filter(|(p, _)| p.ends_with(".md"))
            .map(|(p, t)| {
                (
                    p.to_string(),
                    ParsedText {
                        text: t.to_string(),
                        front_matter: crate::frontmatter::parse(t),
                    },
                )
            })
            .collect()
    }

    fn issues_with(
        roots: &[&str],
        exclude: &[&str],
        files: &[(&str, &str)],
    ) -> BTreeSet<IndexIssue> {
        let mut config = Config::default_config();
        config.index = roots.iter().map(|s| s.to_string()).collect();
        config.exclude = exclude.iter().map(|s| s.to_string()).collect();
        let names: Vec<String> = files.iter().map(|(p, _)| p.to_string()).collect();
        collect_issues(&config, &names, &texts_of(files))
    }

    fn issues(files: &[(&str, &str)]) -> BTreeSet<IndexIssue> {
        issues_with(&["docs"], &[], files)
    }

    fn has(set: &BTreeSet<IndexIssue>, reason: ReasonCode, target: &str) -> bool {
        set.iter().any(|i| i.reason == reason && i.target == target)
    }

    fn count(set: &BTreeSet<IndexIssue>, reason: ReasonCode, target: &str) -> usize {
        set.iter()
            .filter(|i| i.reason == reason && i.target == target)
            .count()
    }

    fn detail_of(set: &BTreeSet<IndexIssue>, reason: ReasonCode, target: &str) -> String {
        set.iter()
            .find(|i| i.reason == reason && i.target == target)
            .unwrap_or_else(|| panic!("no {reason:?} for {target:?} in {set:?}"))
            .detail
            .clone()
    }

    fn fm(text: &str) -> FrontMatter {
        crate::frontmatter::parse(text).unwrap().unwrap()
    }

    /// `update_index` for `docs/index.md` (root `docs`); every file in `files`
    /// exists on disk.
    fn upd(text: &str, documents: &[Document], files: &[&str], unlinked: &[&str]) -> String {
        let unlinked: Vec<String> = unlinked.iter().map(|s| s.to_string()).collect();
        update_index(
            text,
            "docs/index.md",
            &tree_of(files),
            documents,
            &RepoFiles::new(files.iter().copied()),
            &unlinked,
            &test_excluded(),
        )
    }

    fn test_excluded() -> Vec<String> {
        crate::source::DEFAULT_EXCLUDED_DIRS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Index tree of root `docs` from the `index.md` files among `files`.
    fn tree_of(files: &[&str]) -> IndexTree {
        IndexTree::new(
            "docs",
            files
                .iter()
                .filter(|f| f.ends_with("/index.md"))
                .map(|f| f.to_string()),
        )
    }

    // ---- FR-068 parse_entries -----------------------------------------

    // [FR-068]
    #[test]
    fn 箇条書き記号とorderedの各形式がエントリとして検出される() {
        let text = "- [a](a.md)\n\n* [b](b.md)\n\n+ [c](c.md)\n\n1. [d](d.md)\n\n2) [e](e.md)\n";
        assert_eq!(dests(text), ["a.md", "b.md", "c.md", "d.md", "e.md"]);
    }

    // [FR-068]
    #[test]
    fn 入れ子の項目も任意の深さでエントリとして検出される() {
        let text = "- [a](a.md)\n  - [b](sub/b.md)\n    - [c](sub/c.md)\n";
        assert_eq!(dests(text), ["a.md", "sub/b.md", "sub/c.md"]);
    }

    // [FR-068]
    #[test]
    fn ルーズリストでも同じ結果になる() {
        let text = "- [a](a.md) — A\n\n- [b](b.md) — B\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].description, "A");
        assert_eq!(es[1].description, "B");
        assert_eq!(&text[es[0].text_span.clone()], "a");
        assert_eq!(&text[es[1].text_span.clone()], "b");
    }

    // [FR-068]
    #[test]
    fn 太字で包まれたリンクで始まる項目はエントリではない() {
        assert!(parse_entries("- **[a](a.md)** — x\n").is_empty());
    }

    // [FR-068]
    #[test]
    fn タスクリストの項目はエントリではない() {
        assert!(parse_entries("- [ ] [a](a.md)\n- [x] [b](b.md)\n").is_empty());
    }

    // [FR-068]
    #[test]
    fn リンクで始まらない項目はエントリではない() {
        assert!(parse_entries("- see [a](a.md)\n- 説明のみ\n").is_empty());
    }

    // [FR-068]
    #[test]
    fn 段落中や見出し中のリンクはエントリではない() {
        let text = "intro [a](a.md)\n\n# [h](h.md)\n\n- [b](b.md)\n";
        assert_eq!(dests(text), ["b.md"]);
    }

    // [FR-068]
    #[test]
    fn 参照形式のリンクで始まる項目もエントリになる() {
        let text = "- [a][ref]\n- [b][]\n- [c]\n\n[ref]: a.md\n[b]: b.md\n[c]: c.md\n";
        let es = parse_entries(text);
        assert_eq!(
            es.iter().map(|e| e.dest.as_str()).collect::<Vec<_>>(),
            ["a.md", "b.md", "c.md"]
        );
        assert_eq!(&text[es[0].text_span.clone()], "a");
        assert_eq!(&text[es[1].text_span.clone()], "b");
        assert_eq!(&text[es[2].text_span.clone()], "c");
    }

    // [FR-068]
    #[test]
    fn リンクのtitle属性は無視される() {
        let text = "- [a](a.md \"tt\") — d\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].dest, "a.md");
        assert_eq!(es[0].description, "d");
    }

    // [FR-068]
    #[test]
    fn 複数行にわたる項目も1つのエントリになる() {
        let text = "- [a](a.md) — one\n  two\n- [b](b.md)\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].dest, "a.md");
    }

    // [FR-068]
    #[test]
    fn bomがあっても同じエントリが検出される() {
        let text = "\u{feff}- [a](a.md) — d\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 1);
        assert_eq!(&text[es[0].text_span.clone()], "a");
        assert_eq!(&text[es[0].tail_span.clone()], " — d");
    }

    // [FR-068]
    #[test]
    fn okf_version等のfrontmatter付き索引でも検出される() {
        let text = "---\nokf_version: 1\n---\n\n- [a](a.md)\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 1);
        assert_eq!(&text[es[0].text_span.clone()], "a");
    }

    // ---- FR-069 entry description -------------------------------------

    // [FR-069]
    #[test]
    fn 空のリンク文は空文字列でspanは開き括弧の直後になる() {
        let text = "- [](x.md)\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].link_text, "");
        let pos = text.find('[').unwrap() + 1;
        assert_eq!(es[0].text_span, pos..pos);
    }

    // [FR-069]
    #[test]
    fn リンク文のコードスパンや入れ子の角括弧はソースのまま扱う() {
        let es = parse_entries("- [`a` b](x.md)\n- [T [x]](y.md)\n");
        assert_eq!(es[0].link_text, "`a` b");
        assert_eq!(es[1].link_text, "T [x]");
    }

    // [FR-069]
    #[test]
    fn 区切り4種を除いて要約を取り出す() {
        for sep in ['-', '–', '—', ':'] {
            let text = format!("- [a](a.md) {sep} desc\n");
            let es = parse_entries(&text);
            assert_eq!(es[0].separator, Some(sep), "{sep}");
            assert_eq!(es[0].description, "desc", "{sep}");
        }
    }

    // [FR-069]
    #[test]
    fn 区切りがなくても要約になり_区切りの二重指定は1文字だけ除く() {
        let es = parse_entries("- [a](a.md) desc\n- [b](b.md) — — x\n");
        assert_eq!(es[0].separator, None);
        assert_eq!(es[0].description, "desc");
        assert_eq!(es[1].separator, Some('—'));
        assert_eq!(es[1].description, "— x");
    }

    // [FR-069]
    #[test]
    fn 要約のないエントリは空の要約になる() {
        let es = parse_entries("- [a](a.md)\n");
        assert_eq!(es[0].description, "");
        assert_eq!(es[0].separator, None);
        assert!(es[0].tail_span.is_empty());
    }

    // [FR-069]
    #[test]
    fn 折り畳み参照の後ろから要約を取り出す() {
        let text = "- [a][] — desc\n\n[a]: a.md\n";
        let es = parse_entries(text);
        assert_eq!(es[0].description, "desc");
        assert_eq!(&text[es[0].tail_span.clone()], " — desc");
    }

    // [FR-069]
    #[test]
    fn 遅延継続行と複数行の要約を含む() {
        let lazy = parse_entries("- [a](a.md) — first\nsecond\n");
        assert_eq!(lazy[0].description, "first\nsecond");
        let indented = parse_entries("- [a](a.md) — first\n  second\n");
        assert_eq!(indented[0].description, "first\n  second");
    }

    // [FR-069]
    #[test]
    fn 要約に入れ子の子項目は含まれない() {
        let text = "- [a](a.md) — d\n  - [b](b.md) — e\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].description, "d");
        assert_eq!(&text[es[0].tail_span.clone()], " — d");
        assert_eq!(es[1].description, "e");
    }

    // [FR-083]
    #[test]
    fn line_spanは先頭行から改行までを含む() {
        let text = "- [a](a.md) — d\r\n  - [b](b.md)\r\n- [c](c.md) — z";
        let es = parse_entries(text);
        assert_eq!(&text[es[0].line_span.clone()], "- [a](a.md) — d\r\n");
        assert_eq!(&text[es[1].line_span.clone()], "  - [b](b.md)\r\n");
        assert_eq!(&text[es[2].line_span.clone()], "- [c](c.md) — z");
    }

    // [FR-069]
    #[test]
    fn 角括弧のエスケープは比較時に戻される() {
        let files = [
            ("docs/index.md", "- [a\\[1\\]](a.md) — x \\] y\n"),
            (
                "docs/a.md",
                "---\ntype: Specification\ntitle: \"a[1]\"\ndescription: \"x ] y\"\n---\n",
            ),
        ];
        assert!(issues(&files).is_empty(), "{:?}", issues(&files));
    }

    // ---- FR-070 resolve -----------------------------------------------

    fn res(index_path: &str, dest: &str, files: &[&str]) -> Resolution {
        resolve(
            index_path,
            "docs",
            dest,
            &RepoFiles::new(files.iter().copied()),
        )
    }

    // [FR-070]
    #[test]
    fn 空とフラグメントのみは解決不能になる() {
        assert_eq!(
            res("docs/index.md", "", &[]),
            Resolution::Unresolvable("empty target")
        );
        assert_eq!(
            res("docs/index.md", "#x", &[]),
            Resolution::Unresolvable("empty target")
        );
    }

    // [FR-070]
    #[test]
    fn 外部リンクは無視される() {
        assert_eq!(
            res("docs/index.md", "https://example.com/a", &[]),
            Resolution::External
        );
        assert_eq!(
            res("docs/index.md", "mailto:a@example.com", &[]),
            Resolution::External
        );
    }

    // [FR-070]
    #[test]
    fn 絶対パスとリポジトリ外と根の外は解決不能になる() {
        assert_eq!(
            res("docs/index.md", "/abs.md", &[]),
            Resolution::Unresolvable("absolute path")
        );
        assert_eq!(
            res("docs/index.md", "../../out.md", &[]),
            Resolution::Unresolvable("escapes repository")
        );
        assert_eq!(
            res("docs/index.md", "../req/x.md", &["req/x.md"]),
            Resolution::Unresolvable("outside index root")
        );
    }

    // [FR-070]
    #[test]
    fn フラグメントを除いてファイルに解決される() {
        assert_eq!(
            res("docs/index.md", "a.md#sec", &["docs/a.md"]),
            Resolution::File("docs/a.md".to_string())
        );
        assert_eq!(
            res("docs/sub/index.md", "../a.md", &["docs/a.md"]),
            Resolution::File("docs/a.md".to_string())
        );
    }

    // [FR-070]
    #[test]
    fn パーセントデコードされ_デコード後のシャープは再分割されない() {
        assert_eq!(
            res("docs/index.md", "%E9%9B%86.md", &["docs/集.md"]),
            Resolution::File("docs/集.md".to_string())
        );
        assert_eq!(
            res("docs/index.md", "a%23b.md", &["docs/a#b.md"]),
            Resolution::File("docs/a#b.md".to_string())
        );
        assert_eq!(
            res("docs/index.md", "100%.md", &["docs/100%.md"]),
            Resolution::File("docs/100%.md".to_string())
        );
    }

    // [FR-070]
    #[test]
    fn ディレクトリを指す3形は同じディレクトリエントリになる() {
        let files = ["docs/sub/index.md", "docs/sub/x.md"];
        let want = Resolution::Directory("docs/sub".to_string());
        assert_eq!(res("docs/index.md", "sub/", &files), want);
        assert_eq!(res("docs/index.md", "sub", &files), want);
        assert_eq!(res("docs/index.md", "sub/index.md", &files), want);
    }

    // [FR-070]
    #[test]
    fn スラッシュなしでも配下にファイルがあればディレクトリになる() {
        assert_eq!(
            res("docs/index.md", "sub", &["docs/sub/x.md"]),
            Resolution::Directory("docs/sub".to_string())
        );
    }

    // [FR-070]
    #[test]
    fn 自身と祖先のディレクトリは解決不能になる() {
        let why = Resolution::Unresolvable("self or ancestor directory");
        assert_eq!(res("docs/index.md", "./", &[]), why);
        assert_eq!(res("docs/index.md", "index.md", &[]), why);
        assert_eq!(res("docs/a/index.md", "../", &[]), why);
        assert_eq!(res("docs/a/index.md", "../index.md", &[]), why);
        assert_eq!(res("docs/a/index.md", "./", &[]), why);
    }

    // [FR-070]
    #[test]
    fn 存在しないファイルは文書エントリとして扱う() {
        assert_eq!(
            res("docs/index.md", "missing.md", &[]),
            Resolution::File("docs/missing.md".to_string())
        );
    }

    // ---- FR-071 documents ----------------------------------------------

    // [FR-071]
    #[test]
    fn titleはフロントマターのtitleを優先し前後の空白を除く() {
        let t = fm("---\ntype: X\ntitle: \"  Front  \"\n---\n# Heading\n");
        assert_eq!(document_title("docs/a.md", &t), "Front");
    }

    // [FR-071]
    #[test]
    fn titleが空なら最初のh1にフォールバックする() {
        let t = fm("---\ntype: X\ntitle: \"  \"\n---\n# Heading\n");
        assert_eq!(document_title("docs/a.md", &t), "Heading");
    }

    // [FR-071]
    #[test]
    fn h1はatx閉じのシャープを含まない() {
        let t = fm("---\ntype: X\n---\n# Hello World #\n");
        assert_eq!(document_title("docs/a.md", &t), "Hello World");
    }

    // [FR-071]
    #[test]
    fn h1はsetext形式でも取れて下線を含まない() {
        let t = fm("---\ntype: X\n---\nHello World\n===\n");
        assert_eq!(document_title("docs/a.md", &t), "Hello World");
    }

    // [FR-071]
    #[test]
    fn h1のコードスパンはソースのまま含む() {
        let t = fm("---\ntype: X\n---\n# `code` and *em*\n");
        assert_eq!(document_title("docs/a.md", &t), "`code` and *em*");
    }

    // [FR-071]
    #[test]
    fn コードブロック内のシャープ行はh1として扱わない() {
        let t = fm("---\ntype: X\n---\n```\n# not heading\n```\n\n# Real\n");
        assert_eq!(document_title("docs/a.md", &t), "Real");
    }

    // [FR-071]
    #[test]
    fn h1以外の見出しは無視されファイル名にフォールバックする() {
        let t = fm("---\ntype: X\n---\n## second level\n");
        assert_eq!(document_title("docs/foo-bar.md", &t), "foo-bar");
    }

    fn collect(files: &[(&str, &str)]) -> RootFiles {
        collect_documents("docs", &texts_of(files))
    }

    // [FR-071]
    #[test]
    fn descriptionの有無と空と改行の扱い() {
        let files = [
            ("docs/none.md", "---\ntype: X\n---\n"),
            ("docs/empty.md", "---\ntype: X\ndescription: \"\"\n---\n"),
            (
                "docs/newline.md",
                "---\ntype: X\ndescription: \"a\\nb\"\n---\n",
            ),
            (
                "docs/folded.md",
                "---\ntype: X\ndescription: >\n  line1\n  line2\n---\n",
            ),
            (
                "docs/block.md",
                "---\ntype: X\ndescription: |\n  line1\n  line2\n---\n",
            ),
            (
                "docs/trim.md",
                "---\ntype: X\ndescription: \"  x  \"\n---\n",
            ),
        ];
        let RootFiles {
            documents: docs,
            untyped,
            ..
        } = collect(&files);
        assert!(untyped.is_empty());
        let d = |p: &str| {
            docs.iter()
                .find(|d| d.path == p)
                .unwrap_or_else(|| panic!("{p}"))
                .description
                .clone()
        };
        assert_eq!(d("docs/none.md"), None);
        assert_eq!(d("docs/empty.md"), None);
        assert_eq!(d("docs/newline.md"), None);
        assert_eq!(d("docs/folded.md"), Some("line1 line2".to_string()));
        assert_eq!(d("docs/block.md"), None);
        assert_eq!(d("docs/trim.md"), Some("x".to_string()));
    }

    // [FR-067]
    #[test]
    fn indexとlogは予約名で文書にならず索引として分類される() {
        let files = [
            ("docs/index.md", "- [a](a.md)\n"),
            ("docs/sub/index.md", "- [b](b.md)\n"),
            ("docs/log.md", "---\ntype: Specification\n---\n"),
            ("docs/a.md", "---\ntype: Note\ntitle: A\n---\n"),
        ];
        let RootFiles {
            documents: docs,
            untyped,
            indexes,
        } = collect(&files);
        assert_eq!(docs.len(), 1, "{docs:?}");
        assert!(untyped.is_empty(), "{untyped:?}");
        let mut indexes = indexes;
        indexes.sort();
        assert_eq!(indexes, ["docs/index.md", "docs/sub/index.md"]);
    }

    // [FR-075]
    #[test]
    fn typeなしとフロントマターなしはuntypedで壊れたフロントマターは無視される() {
        let files = [
            ("docs/plain.md", "# no frontmatter\n"),
            ("docs/notype.md", "---\ntitle: x\n---\n"),
            ("docs/broken.md", "---\ntype: [unclosed\n---\n"),
        ];
        let RootFiles {
            documents: docs,
            untyped,
            ..
        } = collect(&files);
        assert!(docs.is_empty(), "{docs:?}");
        let mut untyped = untyped;
        untyped.sort();
        assert_eq!(untyped, ["docs/notype.md", "docs/plain.md"]);
    }

    // [FR-071]
    #[test]
    fn typeは任意の値でも文書になる() {
        let files = [("docs/a.md", "---\ntype: Note\ntitle: A\n---\n")];
        let RootFiles {
            documents: docs, ..
        } = collect(&files);
        assert_eq!(docs.len(), 1, "{docs:?}");
        assert_eq!(docs[0].path, "docs/a.md");
        assert_eq!(docs[0].title, "A");
    }

    // ---- FR-072〜079 collect_issues ------------------------------------

    // [FR-072]
    #[test]
    fn 根のみの索引で整合していればfindingは出ない() {
        let a = typed("A", Some("D1"));
        let b = typed("B", Some("D2"));
        let files = [
            ("docs/index.md", "- [A](a.md) — D1\n- [B](b.md) - D2\n"),
            ("docs/a.md", a.as_str()),
            ("docs/b.md", b.as_str()),
        ];
        assert!(issues(&files).is_empty(), "{:?}", issues(&files));
    }

    // [FR-072]
    #[test]
    fn 入れ子の索引で子の文書は子の索引が担当する() {
        let a = typed("A", Some("DA"));
        let b = typed("B", Some("DB"));
        let files = [
            ("docs/index.md", "- [A](a.md) — DA\n- [sub](sub/) — S\n"),
            ("docs/sub/index.md", "- [B](b.md) — DB\n"),
            ("docs/a.md", a.as_str()),
            ("docs/sub/b.md", b.as_str()),
        ];
        assert!(issues(&files).is_empty(), "{:?}", issues(&files));
    }

    // [FR-072]
    #[test]
    fn 親が子の担当文書を直接列挙しても_同じ文書の重複列挙もfindingにならない() {
        let a = typed("A", Some("DA"));
        let b = typed("B", Some("DB"));
        let files = [
            (
                "docs/index.md",
                "- [A](a.md) — DA\n- [A](a.md) — DA\n- [sub](sub/index.md) — S\n- [B](sub/b.md) — DB\n",
            ),
            ("docs/sub/index.md", "- [B](b.md) — DB\n"),
            ("docs/a.md", a.as_str()),
            ("docs/sub/b.md", b.as_str()),
        ];
        assert!(issues(&files).is_empty(), "{:?}", issues(&files));
    }

    // [FR-074]
    #[test]
    fn 担当索引が列挙していなければ他の索引が列挙していてもnot_listedになる() {
        let a = typed("A", Some("DA"));
        let b = typed("B", Some("DB"));
        let files = [
            (
                "docs/index.md",
                "- [A](a.md) — DA\n- [sub](sub/) — S\n- [B](sub/b.md) — DB\n",
            ),
            ("docs/sub/index.md", "intro only\n"),
            ("docs/a.md", a.as_str()),
            ("docs/sub/b.md", b.as_str()),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::NotListed, "docs/sub/b.md"), "{set:?}");
        assert!(!has(&set, ReasonCode::NotListed, "docs/a.md"));
    }

    // [FR-072]
    #[test]
    fn 中間ディレクトリに索引がなくても最も近い祖先の索引が親になる() {
        let c = typed("C", Some("DC"));
        let files = [
            ("docs/index.md", "- [a/b](a/b/) — AB\n"),
            ("docs/a/b/index.md", "- [C](c.md) — DC\n"),
            ("docs/a/b/c.md", c.as_str()),
        ];
        assert!(issues(&files).is_empty(), "{:?}", issues(&files));
    }

    // [FR-072]
    #[test]
    fn 自前の索引を持たない中間ディレクトリへのエントリはbroken_entryになる() {
        let c = typed("C", Some("DC"));
        let files = [
            ("docs/index.md", "- [a](a/) — A\n"),
            ("docs/a/b/index.md", "- [C](c.md) — DC\n"),
            ("docs/a/b/c.md", c.as_str()),
        ];
        let set = issues(&files);
        assert_eq!(
            detail_of(&set, ReasonCode::BrokenEntry, "docs/index.md -> a/"),
            "directory has no index.md"
        );
        assert!(has(&set, ReasonCode::UnlinkedIndex, "docs/a/b/index.md"));
    }

    // [FR-072]
    #[test]
    fn 設定した根が配下のディレクトリでも担当範囲はその根の中だけになる() {
        let g = typed("G", Some("DG"));
        let files = [
            ("docs/guide/index.md", "- [G](g.md) — DG\n"),
            ("docs/guide/g.md", g.as_str()),
            ("docs/other.md", "---\ntype: X\n---\n"),
            ("docs/untyped.md", "plain\n"),
        ];
        let set = issues_with(&["docs/guide"], &[], &files);
        assert!(set.is_empty(), "{set:?}");
    }

    // [FR-073]
    #[test]
    fn 根の索引がなく文書があればmissing_indexで他の検査は省く() {
        let a = typed("A", Some("DA"));
        let files = [
            ("docs/a.md", a.as_str()),
            ("docs/nodesc.md", "---\ntype: X\ntitle: N\n---\n"),
            ("docs/sub/index.md", "- [gone](gone.md)\n"),
            ("docs/u.md", "plain\n"),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::MissingIndex, "docs"), "{set:?}");
        assert!(has(&set, ReasonCode::UntypedFile, "docs/u.md"));
        for r in [
            ReasonCode::NotListed,
            ReasonCode::BrokenEntry,
            ReasonCode::UnlinkedIndex,
            ReasonCode::TitleMismatch,
        ] {
            assert!(set.iter().all(|i| i.reason != r), "{r:?} in {set:?}");
        }
    }

    // [FR-073]
    #[test]
    fn 文書が0件の根ではmissing_indexを出さない() {
        assert!(issues(&[]).is_empty());
        assert!(issues(&[("docs/log.md", "---\ntype: X\n---\n")]).is_empty());
        let set = issues(&[("docs/u.md", "plain\n")]);
        assert!(!set.iter().any(|i| i.reason == ReasonCode::MissingIndex));
        assert!(has(&set, ReasonCode::UntypedFile, "docs/u.md"));
    }

    // [FR-074]
    #[test]
    fn 列挙されていない文書はnot_listedになりidの妥当性に依存しない() {
        let files = [
            ("docs/index.md", "intro\n"),
            (
                "docs/a.md",
                "---\ntype: X\nid: \"bad id!\"\ndescription: D\n---\n",
            ),
            ("docs/b.md", "---\ntype: X\ndescription: D\n---\n"),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::NotListed, "docs/a.md"), "{set:?}");
        assert!(has(&set, ReasonCode::NotListed, "docs/b.md"));
        assert_eq!(count(&set, ReasonCode::NotListed, "docs/a.md"), 1);
    }

    // [FR-075]
    #[test]
    fn typeのないmdはuntyped_fileでindexとlogと根の外は対象外() {
        let files = [
            ("docs/index.md", "- [x](x.md)\n"),
            ("docs/log.md", "plain\n"),
            ("docs/plain.md", "no frontmatter\n"),
            ("docs/notype.md", "---\ntitle: T\n---\n"),
            ("req/outside.md", "plain\n"),
        ];
        let set = issues(&files);
        assert!(
            has(&set, ReasonCode::UntypedFile, "docs/plain.md"),
            "{set:?}"
        );
        assert!(has(&set, ReasonCode::UntypedFile, "docs/notype.md"));
        assert!(!has(&set, ReasonCode::UntypedFile, "docs/index.md"));
        assert!(!has(&set, ReasonCode::UntypedFile, "docs/log.md"));
        assert!(!has(&set, ReasonCode::UntypedFile, "req/outside.md"));
    }

    // [FR-076]
    #[test]
    fn broken_entryのdetailとtargetの正準形() {
        let t = typed("T", Some("D"));
        let index = [
            "- [e]()\n",
            "- [a](#x)\n",
            "- [b](/abs.md)\n",
            "- [c](../../out.md)\n",
            "- [d](../req/x.md)\n",
            "- [s](./)\n",
            "- [f](missing.md)\n",
            "- [g](untyped.md)\n",
            "- [h](dirnoindex/)\n",
            "- [i](pic.png)\n",
            "- [j](vendor/x.md)\n",
            "- [k](node_modules/x.md)\n",
            "- [ext](https://example.com/x)\n",
            "- [mail](mailto:a@example.com)\n",
        ]
        .concat();
        let files = [
            ("docs/index.md", index.as_str()),
            ("docs/untyped.md", "plain\n"),
            ("docs/pic.png", ""),
            ("docs/dirnoindex/x.md", t.as_str()),
        ];
        let set = issues_with(&["docs"], &["vendor"], &files);
        let b = |dest: &str| {
            detail_of(
                &set,
                ReasonCode::BrokenEntry,
                &format!("docs/index.md -> {dest}"),
            )
        };
        assert_eq!(b(""), "empty target");
        assert_eq!(b("#x"), "empty target");
        assert_eq!(b("/abs.md"), "absolute path");
        assert_eq!(b("../../out.md"), "escapes repository");
        assert_eq!(b("../req/x.md"), "outside index root");
        assert_eq!(b("./"), "self or ancestor directory");
        assert_eq!(b("missing.md"), "target does not exist");
        assert_eq!(b("untyped.md"), "target has no `type`");
        assert_eq!(b("dirnoindex/"), "directory has no index.md");
        assert_eq!(b("pic.png"), "target is not a document");
        assert_eq!(b("vendor/x.md"), "target is excluded from scan");
        assert_eq!(b("node_modules/x.md"), "target is excluded from scan");
        assert!(!set.iter().any(|i| i.target.contains("example.com")));
    }

    // [FR-077]
    #[test]
    fn タイトルが一致しなければtitle_mismatchで対象は文書のパス() {
        let a = typed("Real", Some("D"));
        let files = [
            ("docs/index.md", "- [Wrong](a.md) — D\n"),
            ("docs/a.md", a.as_str()),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::TitleMismatch, "docs/a.md"), "{set:?}");
        assert!(set
            .iter()
            .all(|i| i.reason != ReasonCode::DescriptionMismatch));
    }

    // [FR-077]
    #[test]
    fn 要約が一致しなければdescription_mismatchで空白は正規化しない() {
        let a = typed("A", Some("x y"));
        let files = [
            ("docs/index.md", "- [A](a.md) — x  y\n"),
            ("docs/a.md", a.as_str()),
        ];
        let set = issues(&files);
        assert!(
            has(&set, ReasonCode::DescriptionMismatch, "docs/a.md"),
            "{set:?}"
        );
        assert!(set.iter().all(|i| i.reason != ReasonCode::TitleMismatch));
    }

    // [FR-077]
    #[test]
    fn 文書にdescriptionがなければdescription_mismatchを出さない() {
        let a = typed("A", None);
        let files = [
            ("docs/index.md", "- [A](a.md) — whatever\n"),
            ("docs/a.md", a.as_str()),
        ];
        let set = issues(&files);
        assert!(set
            .iter()
            .all(|i| i.reason != ReasonCode::DescriptionMismatch));
        assert!(has(&set, ReasonCode::MissingDescription, "docs/a.md"));
    }

    // [FR-077]
    #[test]
    fn 同じ文書を指す複数エントリはすべて比較され_findingは重複しない() {
        let a = typed("A", Some("D"));
        let files = [
            (
                "docs/index.md",
                "- [A](a.md) — D\n- [X](a.md) — D\n- [Y](a.md) — D\n",
            ),
            ("docs/a.md", a.as_str()),
        ];
        let set = issues(&files);
        assert_eq!(
            count(&set, ReasonCode::TitleMismatch, "docs/a.md"),
            1,
            "{set:?}"
        );
    }

    // [FR-078]
    #[test]
    fn descriptionがない_空_改行入りの文書はmissing_description() {
        let files = [
            ("docs/index.md", "- [A](a.md)\n- [B](b.md)\n- [C](c.md)\n"),
            ("docs/a.md", "---\ntype: X\ntitle: A\n---\n"),
            (
                "docs/b.md",
                "---\ntype: X\ntitle: B\ndescription: \"\"\n---\n",
            ),
            (
                "docs/c.md",
                "---\ntype: X\ntitle: C\ndescription: |\n  l1\n  l2\n---\n",
            ),
        ];
        let set = issues(&files);
        for p in ["docs/a.md", "docs/b.md", "docs/c.md"] {
            assert!(has(&set, ReasonCode::MissingDescription, p), "{p}: {set:?}");
        }
    }

    // [FR-078]
    #[test]
    fn missing_descriptionは未列挙の文書にも出る() {
        let files = [
            ("docs/index.md", "intro\n"),
            ("docs/a.md", "---\ntype: X\ntitle: A\n---\n"),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::MissingDescription, "docs/a.md"));
        assert!(has(&set, ReasonCode::NotListed, "docs/a.md"));
    }

    // [FR-079]
    #[test]
    fn ディレクトリエントリの要約が空ならmissing_directory_description() {
        let b = typed("B", Some("DB"));
        let files = [
            ("docs/index.md", "- [sub](sub/)\n"),
            ("docs/sub/index.md", "- [B](b.md) — DB\n"),
            ("docs/sub/b.md", b.as_str()),
        ];
        let set = issues(&files);
        assert!(
            has(
                &set,
                ReasonCode::MissingDirectoryDescription,
                "docs/index.md -> sub/"
            ),
            "{set:?}"
        );
        assert!(set.iter().all(|i| i.reason != ReasonCode::UnlinkedIndex));
    }

    // [FR-079]
    #[test]
    fn 要約付きのディレクトリエントリは問題にならない() {
        let b = typed("B", Some("DB"));
        let files = [
            ("docs/index.md", "- [sub](sub/) — S\n"),
            ("docs/sub/index.md", "- [B](b.md) — DB\n"),
            ("docs/sub/b.md", b.as_str()),
        ];
        assert!(issues(&files).is_empty());
    }

    // [FR-079]
    #[test]
    fn 親から辿れない子の索引はunlinked_index() {
        let b = typed("B", Some("DB"));
        let files = [
            ("docs/index.md", "intro\n"),
            ("docs/sub/index.md", "- [B](b.md) — DB\n"),
            ("docs/sub/b.md", b.as_str()),
        ];
        let set = issues(&files);
        assert!(
            has(&set, ReasonCode::UnlinkedIndex, "docs/sub/index.md"),
            "{set:?}"
        );
    }

    // [FR-079]
    #[test]
    fn 中間索引を飛ばしたディレクトリエントリも要約がなければmissing_directory_description() {
        let c = typed("C", Some("DC"));
        let files = [
            ("docs/index.md", "- [a/b](a/b/)\n"),
            ("docs/a/b/index.md", "- [C](c.md) — DC\n"),
            ("docs/a/b/c.md", c.as_str()),
        ];
        let set = issues(&files);
        assert!(
            has(
                &set,
                ReasonCode::MissingDirectoryDescription,
                "docs/index.md -> a/b/"
            ),
            "{set:?}"
        );
        assert!(set.iter().all(|i| i.reason != ReasonCode::UnlinkedIndex));
    }

    // ---- FR-081 update_index rewrite -----------------------------------

    // [FR-081]
    #[test]
    fn リンク文と要約を文書の値で置き換え区切りは既存を保つ() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let files = ["docs/a.md"];
        assert_eq!(
            upd("- [Old](a.md) - old\n", &docs, &files, &[]),
            "- [A](a.md) - D\n"
        );
        assert_eq!(
            upd("- [Old](a.md) : old\n", &docs, &files, &[]),
            "- [A](a.md) : D\n"
        );
        assert_eq!(
            upd("- [Old](a.md)\n", &docs, &files, &[]),
            "- [A](a.md) — D\n"
        );
    }

    // [FR-081]
    #[test]
    fn descriptionのない文書は区切りも要約も付けない() {
        let docs = [doc("docs/a.md", "A", None)];
        assert_eq!(
            upd("- [Old](a.md) — x\n", &docs, &["docs/a.md"], &[]),
            "- [A](a.md)\n"
        );
    }

    // [FR-081]
    #[test]
    fn リンク先のソースは変更しない() {
        let docs = [doc("docs/a b.md", "A", Some("D"))];
        let files = ["docs/a b.md"];
        assert_eq!(
            upd("- [Old](<a b.md> \"t\") — x\n", &docs, &files, &[]),
            "- [A](<a b.md> \"t\") — D\n"
        );
        assert_eq!(
            upd("- [Old](a%20b.md) — x\n", &docs, &files, &[]),
            "- [A](a%20b.md) — D\n"
        );
    }

    // [FR-081]
    #[test]
    fn 参照形式の定義とラベルは変更しない() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let files = ["docs/a.md"];
        assert_eq!(
            upd("- [Old][r] — x\n\n[r]: a.md\n", &docs, &files, &[]),
            "- [A][r] — D\n\n[r]: a.md\n"
        );
        // タイトルがラベルと同じなら折り畳み・短縮形も壊れない
        let docs = [doc("docs/a.md", "Old", Some("D"))];
        assert_eq!(
            upd("- [Old][] — x\n\n[Old]: a.md\n", &docs, &files, &[]),
            "- [Old][] — D\n\n[Old]: a.md\n"
        );
        assert_eq!(
            upd("- [Old] — x\n\n[Old]: a.md\n", &docs, &files, &[]),
            "- [Old] — D\n\n[Old]: a.md\n"
        );
    }

    // [FR-081]
    #[test]
    fn タイトルの角括弧はエスケープされる() {
        let docs = [doc("docs/a.md", "a[1]", Some("D"))];
        assert_eq!(
            upd("- [Old](a.md) — x\n", &docs, &["docs/a.md"], &[]),
            "- [a\\[1\\]](a.md) — D\n"
        );
    }

    // [FR-081]
    #[test]
    fn 複数行エントリは1行に畳まれる() {
        let docs = [doc("docs/a.md", "A", Some("first second"))];
        let text = "- [A](a.md) — first\n  second\n- [B](b.md) — DB\n";
        let docs = [docs[0].clone(), doc("docs/b.md", "B", Some("DB"))];
        assert_eq!(
            upd(text, &docs, &["docs/a.md", "docs/b.md"], &[]),
            "- [A](a.md) — first second\n- [B](b.md) — DB\n"
        );
    }

    // [FR-081]
    #[test]
    fn 同じ文書を指す重複エントリはすべて書き換える() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        assert_eq!(
            upd(
                "- [x](a.md) — 1\n- [y](./a.md) — 2\n",
                &docs,
                &["docs/a.md"],
                &[]
            ),
            "- [A](a.md) — D\n- [A](./a.md) — D\n"
        );
    }

    // [FR-081]
    #[test]
    fn すでに一致していればバイト列は変わらない() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let text = "- [A](a.md) — D\n";
        assert_eq!(upd(text, &docs, &["docs/a.md"], &[]), text);
    }

    // [FR-081]
    #[test]
    fn bomは書き換え後も保存される() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        assert_eq!(
            upd("\u{feff}- [Old](a.md) — x\n", &docs, &["docs/a.md"], &[]),
            "\u{feff}- [A](a.md) — D\n"
        );
    }

    // ---- FR-082 append --------------------------------------------------

    // [FR-082]
    #[test]
    fn 未列挙の文書がパス順に末尾へ追加される() {
        let docs = [
            doc("docs/c.md", "C", None),
            doc("docs/a.md", "A", Some("DA")),
            doc("docs/b.md", "B", Some("DB")),
        ];
        let text = "- [A](a.md) — DA\n";
        let files = ["docs/a.md", "docs/b.md", "docs/c.md"];
        assert_eq!(
            upd(text, &docs, &files, &[]),
            "- [A](a.md) — DA\n- [B](b.md) — DB\n- [C](c.md)\n"
        );
    }

    // [FR-082]
    #[test]
    fn 追加するパスは相対でエンコードされタイトルはエスケープされる() {
        let docs = [doc("docs/sub/a b(c)<d>%e#f\\g.md", "t[1]", Some("D"))];
        let files = ["docs/sub/a b(c)<d>%e#f\\g.md", "docs/x.md"];
        assert_eq!(
            upd("- [x](x.md)\n", &docs, &files, &[]),
            "- [x](x.md)\n- [t\\[1\\]](sub/a%20b%28c%29%3Cd%3E%25e%23f%5Cg.md) — D\n"
        );
    }

    // [FR-082]
    #[test]
    fn 末尾の改行と空行の規則() {
        let docs = [doc("docs/b.md", "B", Some("DB"))];
        let files = ["docs/b.md", "docs/x.md"];
        let item = "- [B](b.md) — DB\n";
        // 箇条書きで終わり改行なし -> 改行のみ足す
        assert_eq!(
            upd("- [x](x.md)", &docs, &files, &[]),
            format!("- [x](x.md)\n{item}")
        );
        // 段落で終わる -> 空行を 1 つ挟む
        assert_eq!(
            upd("# Index\n\nintro\n", &docs, &files, &[]),
            format!("# Index\n\nintro\n\n{item}")
        );
        assert_eq!(upd("intro", &docs, &files, &[]), format!("intro\n\n{item}"));
        // 順序付き・別記号の項目で終わる -> 空行なし
        assert_eq!(upd("1. x\n", &docs, &files, &[]), format!("1. x\n{item}"));
        assert_eq!(upd("* x\n", &docs, &files, &[]), format!("* x\n{item}"));
        // 本文が空 -> そのまま
        assert_eq!(upd("", &docs, &files, &[]), item);
    }

    // [FR-082]
    #[test]
    fn 改行コードは既存本文にcrlfがあればcrlfになる() {
        let docs = [doc("docs/b.md", "B", Some("DB"))];
        assert_eq!(
            upd("- [x](x.md)\r\n", &docs, &["docs/b.md", "docs/x.md"], &[]),
            "- [x](x.md)\r\n- [B](b.md) — DB\r\n"
        );
        assert_eq!(
            upd("# T\r\n\r\nintro\r\n", &docs, &["docs/b.md"], &[]),
            "# T\r\n\r\nintro\r\n\r\n- [B](b.md) — DB\r\n"
        );
    }

    // [FR-082]
    #[test]
    fn 辿れない子の索引はディレクトリエントリとして文書の後に追加される() {
        let docs = [doc("docs/a.md", "A", Some("DA"))];
        let out = upd(
            "- [x](x.md)\n",
            &docs,
            &["docs/a.md", "docs/x.md"],
            &["docs/zeta/index.md", "docs/sub/index.md"],
        );
        assert_eq!(
            out,
            "- [x](x.md)\n- [A](a.md) — DA\n- [sub](sub/) — \n- [zeta](zeta/) — \n"
        );
    }

    // ---- FR-083 removal --------------------------------------------------

    // [FR-083]
    #[test]
    fn 存在しない文書へのエントリ行は削除される() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let text = "- [A](a.md) — D\n- [G](gone.md) — g\n- [A](a.md) — D\n";
        assert_eq!(
            upd(text, &docs, &["docs/a.md"], &[]),
            "- [A](a.md) — D\n- [A](a.md) — D\n"
        );
    }

    // [FR-083]
    #[test]
    fn 削除した項目の子項目は残る() {
        let docs = [
            doc("docs/a.md", "A", Some("D")),
            doc("docs/b.md", "B", Some("DB")),
        ];
        let files = ["docs/a.md", "docs/b.md"];
        let text = "- [G](gone.md) — g\n  - [A](a.md) — D\n- [B](b.md) — DB\n";
        assert_eq!(
            upd(text, &docs, &files, &[]),
            "  - [A](a.md) — D\n- [B](b.md) — DB\n"
        );
    }

    // [FR-083]
    #[test]
    fn 削除時もbomは保存される() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        assert_eq!(
            upd(
                "\u{feff}- [G](gone.md)\n- [A](a.md) — D\n",
                &docs,
                &["docs/a.md"],
                &[]
            ),
            "\u{feff}- [A](a.md) — D\n"
        );
    }

    // [FR-083]
    #[test]
    fn 除外配下_typeなし_解決不能_ディレクトリは削除しない() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let text = "- [A](a.md) — D\n\
                    - [n](node_modules/x.md) — 1\n\
                    - [u](untyped.md) — 2\n\
                    - [o](../req/x.md) — 3\n\
                    - [d](sub/) — 4\n\
                    - [e](#x) — 5\n";
        let files = ["docs/a.md", "docs/untyped.md"];
        assert_eq!(upd(text, &docs, &files, &[]), text);
    }

    // ---- FR-084 create ---------------------------------------------------

    // [FR-084]
    #[test]
    fn 索引がなければ見出しなしの平坦なリストを作る() {
        let docs = [
            doc("docs/sub/b.md", "B", None),
            doc("docs/a.md", "A", Some("DA")),
        ];
        let out = upd("", &docs, &["docs/a.md", "docs/sub/b.md"], &[]);
        assert_eq!(out, "- [A](a.md) — DA\n- [B](sub/b.md)\n");
        assert!(!out.contains('#'));
    }

    // ---- FR-085 preserve / idempotent -----------------------------------

    // [FR-085]
    #[test]
    fn エントリ行以外のバイト列は変更されない() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let files = ["docs/a.md", "docs/sub/index.md"];
        let text = "\u{feff}# Index\r\n\r\nintro  text\r\n\r\n## Sec\r\n\r\n- [Old](a.md) — x\r\n  - child [c](c.md)\r\n- **[b](b.md)** — bold\r\n- [sub](sub/) — S\r\n";
        let want = "\u{feff}# Index\r\n\r\nintro  text\r\n\r\n## Sec\r\n\r\n- [A](a.md) — D\r\n  - child [c](c.md)\r\n- **[b](b.md)** — bold\r\n- [sub](sub/) — S\r\n";
        assert_eq!(upd(text, &docs, &files, &[]), want);
    }

    // [FR-085]
    #[test]
    fn 更新は冪等で2回目は同一文字列になる() {
        let docs = [
            doc("docs/a.md", "A", Some("D")),
            doc("docs/b.md", "B[1]", Some("DB")),
            doc("docs/c d.md", "C", None),
        ];
        let files = ["docs/a.md", "docs/b.md", "docs/c d.md"];
        let text = "# Index\n\n- [Old](a.md) - x\n  y\n- [G](gone.md)\n";
        let once = upd(text, &docs, &files, &[]);
        let twice = upd(&once, &docs, &files, &[]);
        assert_eq!(once, twice);
    }

    // ---- boundary tests added for review ---------------------------------

    // [FR-069]
    #[test]
    fn 参照形式とショートカットの後ろから要約を切り出す() {
        let text = "- [T][ref] — 要約1\n- [U] — 要約2\n\n[ref]: a.md\n[U]: b.md\n";
        let es = parse_entries(text);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].description, "要約1");
        assert_eq!(es[1].description, "要約2");
    }

    // [FR-070]
    #[test]
    fn パーセントエンコードされたドットドットはデコード後に正規化される() {
        assert_eq!(
            res("docs/sub/index.md", "%2E%2E/a.md", &["docs/a.md"]),
            Resolution::File("docs/a.md".to_string())
        );
        assert_eq!(
            res("docs/index.md", "%2E%2E/%2E%2E/x.md", &[]),
            Resolution::Unresolvable("escapes repository")
        );
    }

    // [FR-070]
    #[test]
    fn フラグメント付きの索引ファイル指定もディレクトリエントリになる() {
        assert_eq!(
            res("docs/index.md", "sub/index.md#frag", &["docs/sub/index.md"]),
            Resolution::Directory("docs/sub".to_string())
        );
    }

    // [FR-071]
    #[test]
    fn setextのレベル2下線はh1として扱わない() {
        let t = fm("---\ntype: X\n---\nIntro\n---\n");
        assert_eq!(document_title("docs/foo.md", &t), "foo");
    }

    // [FR-071]
    #[test]
    fn 改行を含むtitleは無いものとしてh1にフォールバックする() {
        let t = fm("---\ntype: X\ntitle: \"a\\nb\"\n---\n# Heading\n");
        assert_eq!(document_title("docs/foo.md", &t), "Heading");
        let t = fm("---\ntype: X\ntitle: \"a\\nb\"\n---\n");
        assert_eq!(document_title("docs/foo.md", &t), "foo");
    }

    // [FR-073]
    #[test]
    fn missing_index時は索引側の6種のfindingがすべて出ない() {
        let a = typed("A", Some("DA"));
        let b = typed("B", Some("DB"));
        let files = [
            ("docs/a.md", a.as_str()),
            (
                "docs/sub/index.md",
                "- [X](../a.md) — other\n- [gone](gone.md)\n- [d](dir/)\n- [e](dir2/)\n",
            ),
            ("docs/sub/dir/index.md", "intro\n"),
            ("docs/sub/dir/b.md", b.as_str()),
            ("docs/sub/dir2/index.md", "intro\n"),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::MissingIndex, "docs"), "{set:?}");
        for r in [
            ReasonCode::NotListed,
            ReasonCode::BrokenEntry,
            ReasonCode::TitleMismatch,
            ReasonCode::DescriptionMismatch,
            ReasonCode::MissingDirectoryDescription,
            ReasonCode::UnlinkedIndex,
        ] {
            assert!(set.iter().all(|i| i.reason != r), "{r:?} in {set:?}");
        }
    }

    // [FR-076]
    #[test]
    fn 索引自身を指すエントリは自己か祖先として壊れている() {
        let a = typed("A", Some("D"));
        let files = [
            (
                "docs/index.md",
                "- [A](a.md) — D\n- [me](index.md)\n- [s](sub/index.md)\n",
            ),
            ("docs/a.md", a.as_str()),
        ];
        let set = issues(&files);
        assert_eq!(
            detail_of(&set, ReasonCode::BrokenEntry, "docs/index.md -> index.md"),
            "self or ancestor directory"
        );
        assert_eq!(
            detail_of(
                &set,
                ReasonCode::BrokenEntry,
                "docs/index.md -> sub/index.md"
            ),
            "directory has no index.md"
        );
    }

    // [FR-077]
    #[test]
    fn 同じ文書を指す2本目だけ不一致でもfindingが出る() {
        let a = typed("A", Some("D"));
        let files = [
            ("docs/index.md", "- [A](a.md) — D\n- [X](a.md) — Y\n"),
            ("docs/a.md", a.as_str()),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::TitleMismatch, "docs/a.md"), "{set:?}");
        assert!(has(&set, ReasonCode::DescriptionMismatch, "docs/a.md"));
    }

    // [FR-081]
    #[test]
    fn 区切りのen_dashは書き戻しで保持される() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        assert_eq!(
            upd("- [Old](a.md) – old\n", &docs, &["docs/a.md"], &[]),
            "- [A](a.md) – D\n"
        );
    }

    // [FR-081]
    #[test]
    fn 折り畳み参照のラベルは変えず要約だけ更新されtitle_mismatchが残る() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let text = "- [旧][] — x\n\n[旧]: a.md\n";
        let out = upd(text, &docs, &["docs/a.md"], &[]);
        assert_eq!(out, "- [旧][] — D\n\n[旧]: a.md\n");
        let a = typed("A", Some("D"));
        let set = issues(&[("docs/index.md", out.as_str()), ("docs/a.md", a.as_str())]);
        assert!(has(&set, ReasonCode::TitleMismatch, "docs/a.md"), "{set:?}");
        assert!(set
            .iter()
            .all(|i| i.reason != ReasonCode::DescriptionMismatch));
    }

    // [FR-083]
    #[test]
    fn 直前に項目がある状態で削除すると子項目は直前の項目の下に残る() {
        let docs = [
            doc("docs/a.md", "A", Some("D")),
            doc("docs/b.md", "B", Some("DB")),
        ];
        let files = ["docs/a.md", "docs/b.md"];
        let text = "- [A](a.md) — D\n- [G](gone.md) — g\n  - [B](b.md) — DB\n";
        assert_eq!(
            upd(text, &docs, &files, &[]),
            "- [A](a.md) — D\n  - [B](b.md) — DB\n"
        );
    }

    // [FR-083]
    #[test]
    fn 複数行エントリの削除で末尾行まで消える() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let text = "- [G](gone.md) — first\n  second line\n- [A](a.md) — D\n";
        assert_eq!(upd(text, &docs, &["docs/a.md"], &[]), "- [A](a.md) — D\n");
    }

    // [FR-085]
    #[test]
    fn 書き込みはrename失敗時に同じディレクトリの一時ファイルを残さない() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("index.md");
        // rename 先がディレクトリなので失敗する。
        std::fs::create_dir(&target).unwrap();
        assert!(write_atomic(&target, "x").is_err());
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["index.md"]);

        let ok = dir.path().join("ok.md");
        write_atomic(&ok, "y").unwrap();
        assert_eq!(std::fs::read_to_string(&ok).unwrap(), "y");
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    // ---- review fixes ----------------------------------------------------

    // [FR-081]
    #[test]
    fn エスケープされた角括弧を含むリンク文は壊れずに冪等に書き換わる() {
        let docs = [doc("docs/a.md", "[x] y", Some("D"))];
        let files = ["docs/a.md"];
        let once = upd("- [\\[x\\]](a.md) — d\n", &docs, &files, &[]);
        assert_eq!(once, "- [\\[x\\] y](a.md) — D\n");
        assert_eq!(upd(&once, &docs, &files, &[]), once);
    }

    // [FR-081]
    #[test]
    fn 各リンク形式でリンク文の範囲は元のソースの閉じ括弧までになる() {
        let e = &parse_entries("- [a\\]b](a.md \"t\") — d\n")[0];
        assert_eq!(e.link_text, "a\\]b");
        let e = &parse_entries("- [a *b*][r] — d\n\n[r]: a.md\n")[0];
        assert_eq!(e.link_text, "a *b*");
        assert_eq!(e.description, "d");
        let e = &parse_entries("- [a\\]][] — d\n\n[a\\]]: a.md\n")[0];
        assert_eq!(e.link_text, "a\\]");
        assert_eq!(e.description, "d");
        let e = &parse_entries("- [a] — d\n\n[a]: a.md\n")[0];
        assert_eq!(e.link_text, "a");
        assert_eq!(e.description, "d");
        let e = &parse_entries("- [](a.md) — d\n")[0];
        assert_eq!(e.link_text, "");
    }

    // [FR-081]
    #[test]
    fn ハードブレークとソフトブレークの直後の要約は1行に畳まれる() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let files = ["docs/a.md"];
        assert_eq!(
            upd("- [a](a.md)  \n  hard\n", &docs, &files, &[]),
            "- [A](a.md) — D\n"
        );
        assert_eq!(
            upd("- [a](a.md)\n  soft\n", &docs, &files, &[]),
            "- [A](a.md) — D\n"
        );
    }

    // [FR-082]
    #[test]
    fn バックスラッシュはエスケープされ再パースで元のタイトルに戻る() {
        let docs = [doc("docs/a.md", "a\\", Some("D"))];
        let out = upd("", &docs, &["docs/a.md"], &[]);
        assert_eq!(out, "- [a\\\\](a.md) — D\n");
        let e = &parse_entries(&out)[0];
        assert_eq!(unescape_brackets(&e.link_text), "a\\");
        assert_eq!(upd(&out, &docs, &["docs/a.md"], &[]), out);
        assert_eq!(unescape_brackets("\\\\ \\[ \\] \\x"), "\\ [ ] \\x");
    }

    // [FR-081]
    #[test]
    fn 要約の角括弧とバックスラッシュもエスケープされ比較で不一致にならない() {
        let docs = [doc("docs/a.md", "T", Some("a [b] c\\d"))];
        let files = ["docs/a.md"];
        let once = upd("- [T](a.md) — old\n", &docs, &files, &[]);
        assert_eq!(once, "- [T](a.md) — a \\[b\\] c\\\\d\n");
        let e = &parse_entries(&once)[0];
        assert_eq!(unescape_brackets(&e.description), "a [b] c\\d");
        assert_eq!(upd(&once, &docs, &files, &[]), once);
    }

    // [FR-083]
    #[test]
    fn 除外名の配下を指す存在しないエントリ行は削除されない() {
        let docs = [doc("docs/a.md", "A", Some("D"))];
        let files = ["docs/a.md"];
        let text = "- [A](a.md) — D\n- [X](vendor/x.md) — x\n- [G](gone.md) — g\n";
        let excluded = vec!["vendor".to_string()];
        let out = update_index(
            text,
            "docs/index.md",
            &tree_of(&files),
            &docs,
            &RepoFiles::new(files.iter().copied()),
            &[],
            &excluded,
        );
        assert_eq!(out, "- [A](a.md) — D\n- [X](vendor/x.md) — x\n");
    }

    // [FR-071]
    #[test]
    fn 最初のh1が空なら次のh1を探さずファイル名にフォールバックする() {
        let t = fm("---\ntype: X\n---\n#\n\n# Second\n");
        assert_eq!(document_title("docs/foo.md", &t), "foo");
    }

    // [FR-069]
    #[test]
    fn 強調を含むリンク文はソース文字列のまま扱われる() {
        let e = &parse_entries("- [*a*](x.md)\n")[0];
        assert_eq!(e.link_text, "*a*");
    }

    // [FR-076]
    #[test]
    fn 山括弧とtitle付きの宛先のbroken_entryのtargetは解釈後のdestinationになる() {
        let files = [
            (
                "docs/index.md",
                "- [x](<gone file.md> \"t\")\n- [a](a.md)\n",
            ),
            ("docs/a.md", &typed("a", Some("d"))),
        ];
        let set = issues(&files);
        assert_eq!(
            detail_of(
                &set,
                ReasonCode::BrokenEntry,
                "docs/index.md -> gone file.md"
            ),
            "target does not exist"
        );
    }

    // [FR-079]
    #[test]
    fn 根が子孫を列挙していても直近の親が列挙していなければunlinked_indexになる() {
        let files = [
            ("docs/index.md", "- [a](a/) — A\n- [a/b](a/b/) — B\n"),
            ("docs/a/index.md", "- [x](x.md)\n"),
            ("docs/a/x.md", &typed("x", Some("d"))),
            ("docs/a/b/index.md", "- [y](y.md)\n"),
            ("docs/a/b/y.md", &typed("y", Some("d"))),
        ];
        let set = issues(&files);
        assert!(has(&set, ReasonCode::UnlinkedIndex, "docs/a/b/index.md"));
        assert!(!has(&set, ReasonCode::UnlinkedIndex, "docs/a/index.md"));
    }

    // [FR-082]
    #[test]
    fn 子索引の追加は直近の親の索引の末尾にだけ行われる() {
        let files = [
            "docs/index.md",
            "docs/a/index.md",
            "docs/a/b/index.md",
            "docs/a/x.md",
        ];
        let tree = tree_of(&files);
        let repo = RepoFiles::new(files.iter().copied());
        let child = vec!["docs/a/b/index.md".to_string()];
        let out = update_index(
            "- [x](x.md)\n",
            "docs/a/index.md",
            &tree,
            &[],
            &repo,
            &child,
            &test_excluded(),
        );
        assert_eq!(out, "- [x](x.md)\n- [b](b/) — \n");
        let root = update_index(
            "- [a](a/) — \n",
            "docs/index.md",
            &tree,
            &[],
            &repo,
            &[],
            &test_excluded(),
        );
        assert_eq!(root, "- [a](a/) — \n");
    }
}
