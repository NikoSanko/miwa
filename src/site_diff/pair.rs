//! Pairing of the documents of the two sides, and the choice of the ones that
//! changed (FR-111, FR-112, FR-113).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::Side;
use crate::site::{DocClass, Kind};

/// What happened to a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Status {
    Del,
    Add,
    Mod,
    Move,
}

/// One document of the diff: its page path on each side that publishes it.
pub(super) struct Item {
    pub status: Status,
    pub base: Option<String>,
    pub head: Option<String>,
    /// The front matter `id` (index pages have none).
    pub id: Option<String>,
}

impl Item {
    /// The path the item is listed under: the head path, else the base path.
    pub(super) fn path(&self) -> &str {
        self.head.as_deref().or(self.base.as_deref()).unwrap_or("")
    }
}

/// A typed document of one side, published or not.
struct Entry<'a> {
    path: &'a str,
    class: DocClass,
    id: Option<&'a str>,
    published: bool,
    title: &'a str,
    description: Option<&'a str>,
    body: &'a str,
}

fn entries<'a>(side: &'a Side) -> Vec<Entry<'a>> {
    let published = side
        .model
        .pages
        .iter()
        .filter(|(_, p)| p.kind != Kind::Index)
        .map(|(path, p)| Entry {
            path,
            class: p.class,
            id: p.front_id.as_deref(),
            published: true,
            title: &p.title,
            description: p.description.as_deref(),
            body: &p.body,
        });
    let unpublished = side.candidates.iter().map(|c| Entry {
        path: &c.path,
        class: c.class,
        id: c.id.as_deref(),
        published: false,
        title: &c.title,
        description: c.description.as_deref(),
        body: &c.body,
    });
    let mut all: Vec<Entry> = published.chain(unpublished).collect();
    all.sort_by(|a, b| a.path.cmp(b.path));
    all
}

/// Fails when a side has two published documents of the same class and `id`
/// (the pairing would not be unique).
fn check_duplicates(entries: &[Entry], label: &str) -> anyhow::Result<()> {
    let mut seen: BTreeMap<(DocClass, &str), Vec<&str>> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.published) {
        if let Some(id) = e.id {
            seen.entry((e.class, id)).or_default().push(e.path);
        }
    }
    let lines: Vec<String> = seen
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|((_, id), paths)| format!("duplicate id {id} in {label}: {}", paths.join(", ")))
        .collect();
    if lines.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{}", lines.join("\n"))
    }
}

/// Pairs the entries of the two sides by (class, id), then by path and class
/// when one of them has no id (FR-112). Indexes into `base` / `head`.
fn pair_entries(base: &[Entry], head: &[Entry]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut by_id: HashMap<(DocClass, &str), usize> = HashMap::new();
    for (i, e) in base.iter().enumerate() {
        if let Some(id) = e.id {
            by_id.entry((e.class, id)).or_insert(i);
        }
    }
    let mut base_used = vec![false; base.len()];
    let mut head_pair: Vec<Option<usize>> = vec![None; head.len()];
    for (j, e) in head.iter().enumerate() {
        let Some(id) = e.id else { continue };
        if let Some(&i) = by_id.get(&(e.class, id)) {
            if !base_used[i] {
                base_used[i] = true;
                head_pair[j] = Some(i);
            }
        }
    }
    let by_path: HashMap<(&str, DocClass), usize> = base
        .iter()
        .enumerate()
        .filter(|(i, _)| !base_used[*i])
        .map(|(i, e)| ((e.path, e.class), i))
        .collect();
    for (j, e) in head.iter().enumerate() {
        if head_pair[j].is_some() {
            continue;
        }
        if let Some(&i) = by_path.get(&(e.path, e.class)) {
            if !base_used[i] && (e.id.is_none() || base[i].id.is_none()) {
                base_used[i] = true;
                head_pair[j] = Some(i);
            }
        }
    }
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = head_pair
        .iter()
        .enumerate()
        .map(|(j, i)| (*i, Some(j)))
        .collect();
    pairs.extend(
        base_used
            .iter()
            .enumerate()
            .filter(|(_, used)| !**used)
            .map(|(i, _)| (Some(i), None)),
    );
    pairs
}

/// Whether the paired documents differ in what the page shows: path, title,
/// description, body and, when both are published, the referenced images.
fn changed(base: &Side, head: &Side, b: &Entry, h: &Entry) -> bool {
    if b.path != h.path || b.title != h.title || b.description != h.description || b.body != h.body
    {
        return true;
    }
    b.published && h.published && base.image_hashes(b.path) != head.image_hashes(h.path)
}

/// The documents that changed, and the ones that must have an `id` but do
/// not. Fails with the list of the latter (FR-112).
pub(super) fn pair_documents(base: &Side, head: &Side) -> anyhow::Result<Vec<Item>> {
    let (be, he) = (entries(base), entries(head));
    check_duplicates(&be, "the base")?;
    check_duplicates(&he, "the head")?;

    let mut items = Vec::new();
    let mut without_id: BTreeSet<&str> = BTreeSet::new();
    for (bi, hi) in pair_entries(&be, &he) {
        let (b, h) = (bi.map(|i| &be[i]), hi.map(|j| &he[j]));
        let differs = match (b, h) {
            (Some(b), Some(h)) => changed(base, head, b, h),
            _ => true,
        };
        let item = match (b.filter(|e| e.published), h.filter(|e| e.published)) {
            (Some(b), Some(h)) if differs => Some(Item {
                status: if b.path == h.path {
                    Status::Mod
                } else {
                    Status::Move
                },
                base: Some(b.path.to_string()),
                head: Some(h.path.to_string()),
                id: h.id.or(b.id).map(str::to_string),
            }),
            (Some(b), None) => Some(Item {
                status: Status::Del,
                base: Some(b.path.to_string()),
                head: None,
                id: b.id.map(str::to_string),
            }),
            (None, Some(h)) => Some(Item {
                status: Status::Add,
                base: None,
                head: Some(h.path.to_string()),
                id: h.id.map(str::to_string),
            }),
            _ => None,
        };
        if let Some(h) = h {
            let lost = b.is_some_and(|b| b.id.is_some());
            if h.id.is_none() && (lost || differs) {
                without_id.insert(h.path);
            }
        }
        items.extend(item);
    }
    if !without_id.is_empty() {
        let list: Vec<&str> = without_id.into_iter().collect();
        anyhow::bail!(
            "documents in the diff have no id; add \"id:\" to their front matter:\n{}",
            list.join("\n")
        );
    }
    Ok(items)
}

/// The index pages that changed, by path (FR-113).
pub(super) fn pair_indexes(base: &Side, head: &Side) -> Vec<Item> {
    let indexes = |side: &Side| -> BTreeMap<String, (String, Option<String>, String)> {
        side.model
            .pages
            .iter()
            .filter(|(_, p)| p.kind == Kind::Index)
            .map(|(path, p)| {
                (
                    path.clone(),
                    (p.title.clone(), p.description.clone(), p.body.clone()),
                )
            })
            .collect()
    };
    let (b, h) = (indexes(base), indexes(head));
    let paths: BTreeSet<&String> = b.keys().chain(h.keys()).collect();
    let mut items = Vec::new();
    for path in paths {
        let status = match (b.get(path), h.get(path)) {
            (Some(x), Some(y)) if x == y => continue,
            (Some(_), Some(_)) => Status::Mod,
            (Some(_), None) => Status::Del,
            _ => Status::Add,
        };
        items.push(Item {
            status,
            base: b.contains_key(path).then(|| path.clone()),
            head: h.contains_key(path).then(|| path.clone()),
            id: None,
        });
    }
    items
}
