//! The frame of one document: blocks aligned side by side, character-level
//! marks, folding of the unchanged rows (FR-116 .. FR-118).

use std::collections::{HashMap, HashSet};

use pulldown_cmark::{html::push_html, CowStr, Event, Tag, TagEnd};

use super::pair::{Item, Status};
use super::Side;
use crate::diff::{self, BlockKey, BlockKind, RowKind};
use crate::site::{esc, rewrite_events, BlockSpan, DiffMode, ListKind, Mode, Page, Rendered};

/// Unchanged rows kept visible around each change (FR-118).
const CONTEXT: usize = 1;
/// Squares in the change bar (FR-116).
const BAR_SQUARES: usize = 5;

/// What the frames of one page share.
pub(super) struct Ctx<'a> {
    pub base: &'a Side<'a>,
    pub head: &'a Side<'a>,
    /// Anchor of each document of the diff, by its path on each side.
    pub base_anchors: HashMap<String, String>,
    pub head_anchors: HashMap<String, String>,
    /// Paths (per side) of the documents shown without content.
    pub base_frameless: HashSet<String>,
    pub head_frameless: HashSet<String>,
    pub link_base: Option<&'a str>,
}

/// The status icon (FR-115).
pub(super) fn icon(status: Status) -> String {
    let (class, glyph) = match status {
        Status::Mod => ("mod", "•"),
        Status::Add => ("add", "+"),
        Status::Del => ("del", "−"),
        Status::Move => ("move", "→"),
    };
    format!("<span class=\"st {class}\">{glyph}</span>")
}

/// The blocks of one side of a document.
struct SideBlocks<'a> {
    /// The events to show (ids and links carry the side's prefix).
    events: Vec<Event<'a>>,
    spans: Vec<BlockSpan>,
    /// Comparison keys for the alignment.
    keys: Vec<BlockKey>,
    has_image: Vec<bool>,
}

/// Row classes, as in the `row` element's second class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowClass {
    Same,
    Mod,
    Add,
    Del,
    Img,
}

/// One row of the body.
struct BodyRow {
    class: RowClass,
    left: Option<String>,
    right: Option<String>,
    /// The heading text, when the row is a heading block.
    heading: Option<String>,
    /// A label for the `title` / `description` rows.
    label: Option<&'static str>,
}

fn text_chars(events: &[Event]) -> Vec<char> {
    let mut out = Vec::new();
    let mut in_image = 0usize;
    for ev in events {
        match ev {
            Event::Start(Tag::Image { .. }) => in_image += 1,
            Event::End(TagEnd::Image) => in_image = in_image.saturating_sub(1),
            Event::Text(t) | Event::Code(t) if in_image == 0 => out.extend(t.chars()),
            _ => {}
        }
    }
    out
}

fn side_blocks<'a>(
    side: &'a Side,
    ctx: &Ctx,
    path: &str,
    anchor: &str,
    page: &'a Page,
) -> SideBlocks<'a> {
    let env = side.model.env();
    let (anchors, frameless) = if side.tag == 'b' {
        (&ctx.base_anchors, &ctx.base_frameless)
    } else {
        (&ctx.head_anchors, &ctx.head_frameless)
    };
    let mut key_src = |image: &str| side.images.hash(image).map(|h| format!("{image}#{h}"));
    let mut key_mode = Mode::Diff(DiffMode {
        own: anchor,
        side: None,
        anchors,
        frameless,
        link_base: ctx.link_base,
        image_src: &mut key_src,
    });
    let key = rewrite_events(
        &env,
        path,
        &page.body,
        "",
        &mut key_mode,
        &mut Rendered::default(),
    );
    let keys: Vec<BlockKey> = key
        .blocks
        .iter()
        .map(|span| {
            let events = &key.events[span.range.clone()];
            let text = if span.kind == BlockKind::Html {
                events
                    .iter()
                    .filter_map(|e| match e {
                        Event::Html(h) => Some(h.to_string()),
                        _ => None,
                    })
                    .collect()
            } else {
                text_chars(events).into_iter().collect()
            };
            BlockKey {
                kind: span.kind,
                key: block_html(events, span.list, None),
                text,
            }
        })
        .collect();
    let has_image = key
        .blocks
        .iter()
        .map(|span| {
            key.events[span.range.clone()]
                .iter()
                .any(|e| matches!(e, Event::Start(Tag::Image { .. })))
        })
        .collect();

    let mut show_src = |image: &str| side.images.data_uri(image);
    let mut show_mode = Mode::Diff(DiffMode {
        own: anchor,
        side: Some(side.tag),
        anchors,
        frameless,
        link_base: ctx.link_base,
        image_src: &mut show_src,
    });
    let shown = rewrite_events(
        &env,
        path,
        &page.body,
        "",
        &mut show_mode,
        &mut Rendered::default(),
    );
    SideBlocks {
        events: shown.events,
        spans: shown.blocks,
        keys,
        has_image,
    }
}

impl SideBlocks<'_> {
    fn len(&self) -> usize {
        self.spans.len().min(self.keys.len())
    }

    fn events_of(&self, i: usize) -> &[Event<'_>] {
        &self.events[self.spans[i].range.clone()]
    }

    fn html(&self, i: usize, marks: Option<(&[bool], &str)>) -> String {
        block_html(self.events_of(i), self.spans[i].list, marks)
    }
}

/// Renders one block; a list item is wrapped in its own list. With `marks`,
/// the changed characters of the text are wrapped in `<span class="...">`.
fn block_html(events: &[Event], list: Option<ListKind>, marks: Option<(&[bool], &str)>) -> String {
    let mut html = String::new();
    match list {
        Some(ListKind::Unordered) => html.push_str("<ul>\n"),
        Some(ListKind::Ordered(n)) => html.push_str(&format!("<ol start=\"{n}\">\n")),
        None => {}
    }
    match marks {
        Some((marks, class)) => push_html(&mut html, mark_events(events, marks, class).into_iter()),
        None => push_html(&mut html, events.iter().cloned()),
    }
    match list {
        Some(ListKind::Unordered) => html.push_str("</ul>\n"),
        Some(ListKind::Ordered(_)) => html.push_str("</ol>\n"),
        None => {}
    }
    html
}

/// Wraps the marked characters of `text` in `<span class="{class}">`, as
/// events that sit inside the text event.
fn push_marked<'a>(out: &mut Vec<Event<'a>>, text: &str, marks: &[bool], class: &str) {
    let flush = |out: &mut Vec<Event<'a>>, run: &mut String, marked: bool| {
        if run.is_empty() {
            return;
        }
        let piece = CowStr::from(std::mem::take(run));
        if marked {
            out.push(Event::Html(format!("<span class=\"{class}\">").into()));
            out.push(Event::Text(piece));
            out.push(Event::Html("</span>".into()));
        } else {
            out.push(Event::Text(piece));
        }
    };
    let mut run = String::new();
    let mut current = false;
    for (c, marked) in text.chars().zip(marks.iter().copied()) {
        if marked != current {
            flush(out, &mut run, current);
            current = marked;
        }
        run.push(c);
    }
    flush(out, &mut run, current);
}

/// Splits the text and code events around the marked characters; inline code
/// is written out by hand so that the spans sit inside `<code>`.
fn mark_events<'a>(events: &[Event<'a>], marks: &[bool], class: &str) -> Vec<Event<'a>> {
    let mut out = Vec::with_capacity(events.len());
    let mut pos = 0usize;
    let mut in_image = 0usize;
    for ev in events {
        match ev {
            Event::Start(Tag::Image { .. }) => {
                in_image += 1;
                out.push(ev.clone());
            }
            Event::End(TagEnd::Image) => {
                in_image = in_image.saturating_sub(1);
                out.push(ev.clone());
            }
            Event::Text(t) if in_image == 0 => {
                let n = t.chars().count();
                push_marked(&mut out, t, marks.get(pos..pos + n).unwrap_or(&[]), class);
                pos += n;
            }
            Event::Code(t) if in_image == 0 => {
                let n = t.chars().count();
                out.push(Event::Html("<code>".into()));
                push_marked(&mut out, t, marks.get(pos..pos + n).unwrap_or(&[]), class);
                out.push(Event::Html("</code>".into()));
                pos += n;
            }
            _ => out.push(ev.clone()),
        }
    }
    out
}

/// A plain text with the marked characters wrapped in `<span class="{class}">`.
fn mark_text(text: &str, marks: &[bool], class: &str) -> String {
    let mut out = String::new();
    let mut open = false;
    for (c, marked) in text.chars().zip(marks.iter().copied()) {
        if marked != open {
            if marked {
                out.push_str(&format!("<span class=\"{class}\">"));
            } else {
                out.push_str("</span>");
            }
            open = marked;
        }
        out.push_str(&esc(&c.to_string()));
    }
    if open {
        out.push_str("</span>");
    }
    out
}

/// A `title` / `description` row, with character marks.
fn meta_row(label: &'static str, old: &str, new: &str) -> BodyRow {
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let (left, right) = match diff::char_marks(&a, &b) {
        Some((ma, mb)) => (mark_text(old, &ma, "dd"), mark_text(new, &mb, "da")),
        None => (esc(old), esc(new)),
    };
    BodyRow {
        class: RowClass::Mod,
        left: Some(format!("<p>{left}</p>")),
        right: Some(format!("<p>{right}</p>")),
        heading: None,
        label: Some(label),
    }
}

fn heading_of(blocks: &SideBlocks, i: usize) -> Option<String> {
    (blocks.keys[i].kind == BlockKind::Heading).then(|| blocks.keys[i].text.clone())
}

/// The aligned rows of the two bodies.
fn body_rows(left: &SideBlocks, right: &SideBlocks) -> Vec<BodyRow> {
    let lk = &left.keys[..left.len()];
    let rk = &right.keys[..right.len()];
    diff::align_blocks(lk, rk)
        .into_iter()
        .map(|row| match (row.kind, row.left, row.right) {
            (RowKind::Same, Some(l), Some(r)) => BodyRow {
                class: RowClass::Same,
                left: Some(left.html(l, None)),
                right: Some(right.html(r, None)),
                heading: heading_of(right, r),
                label: None,
            },
            (RowKind::Changed, Some(l), Some(r)) => changed_row(left, right, l, r),
            (RowKind::Added, _, Some(r)) => BodyRow {
                class: RowClass::Add,
                left: None,
                right: Some(right.html(r, None)),
                heading: heading_of(right, r),
                label: None,
            },
            (_, Some(l), _) => BodyRow {
                class: RowClass::Del,
                left: Some(left.html(l, None)),
                right: None,
                heading: heading_of(left, l),
                label: None,
            },
            _ => unreachable!("an aligned row has a block on at least one side"),
        })
        .collect()
}

/// A paired block: character marks inside the text, none when only the HTML
/// differs or the pair is too large.
fn changed_row(left: &SideBlocks, right: &SideBlocks, l: usize, r: usize) -> BodyRow {
    let (lc, rc) = (
        text_chars(left.events_of(l)),
        text_chars(right.events_of(r)),
    );
    let (lhtml, rhtml) = match diff::char_marks(&lc, &rc) {
        Some((ma, mb)) => (
            left.html(l, Some((&ma, "dd"))),
            right.html(r, Some((&mb, "da"))),
        ),
        None => (left.html(l, None), right.html(r, None)),
    };
    let image_only = lc.is_empty() && rc.is_empty() && (left.has_image[l] || right.has_image[r]);
    BodyRow {
        class: if image_only {
            RowClass::Img
        } else {
            RowClass::Mod
        },
        left: Some(lhtml),
        right: Some(rhtml),
        heading: heading_of(right, r),
        label: None,
    }
}

fn row_html(row: &BodyRow) -> String {
    let class = match row.class {
        RowClass::Same => "same",
        RowClass::Mod => "mod",
        RowClass::Add => "add",
        RowClass::Del => "del",
        RowClass::Img => "img",
    };
    let left_glyph = if matches!(row.class, RowClass::Mod | RowClass::Del | RowClass::Img) {
        "−"
    } else {
        ""
    };
    let right_glyph = if matches!(row.class, RowClass::Mod | RowClass::Add | RowClass::Img) {
        "+"
    } else {
        ""
    };
    let label = row
        .label
        .map(|l| format!("<span class=\"lab\">{l}</span>"))
        .unwrap_or_default();
    let cell = |side: char, html: &Option<String>| match html {
        Some(h) => format!("<div class=\"c {side}\">{label}{h}</div>"),
        None => format!("<div class=\"c {side} void\"></div>"),
    };
    format!(
        "<div class=\"row {class}\"><div class=\"g l\">{left_glyph}</div>{}<div class=\"g r\">{right_glyph}</div>{}</div>\n",
        cell('l', &row.left),
        cell('r', &row.right),
    )
}

/// The rows, with the unchanged ones away from a change folded (FR-118).
fn folded_html(rows: &[BodyRow]) -> String {
    let mut keep = vec![false; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        if row.class != RowClass::Same {
            let from = i.saturating_sub(CONTEXT);
            let to = (i + CONTEXT).min(rows.len() - 1);
            keep[from..=to].fill(true);
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < rows.len() {
        if keep[i] {
            out.push_str(&row_html(&rows[i]));
            i += 1;
            continue;
        }
        let start = i;
        while i < rows.len() && !keep[i] {
            i += 1;
        }
        // The last heading in the range, else the closest one before it.
        let heading = rows[..i].iter().rev().find_map(|r| r.heading.as_deref());
        let section = heading
            .map(|h| format!(" § {}", esc(h)))
            .unwrap_or_default();
        out.push_str(&format!(
            "<details class=\"fold\"><summary>↕ {}{section}</summary>\n",
            i - start
        ));
        for row in &rows[start..i] {
            out.push_str(&row_html(row));
        }
        out.push_str("</details>\n");
    }
    out
}

/// `(green, red)` squares for `a` added and `d` deleted rows (FR-116).
fn bar_squares(a: usize, d: usize) -> (usize, usize) {
    if a + d == 0 {
        return (0, 0);
    }
    let share = |n: usize| (2 * BAR_SQUARES * n + (a + d)) / (2 * (a + d));
    let (mut green, mut red) = (share(a), share(d));
    if a > 0 {
        green = green.max(1);
    }
    if d > 0 {
        red = red.max(1);
    }
    while green + red > BAR_SQUARES {
        if green >= red {
            green -= 1;
        } else {
            red -= 1;
        }
    }
    (green, red)
}

fn bar_html(classes: &[RowClass]) -> String {
    let count = |wanted: &[RowClass]| classes.iter().filter(|c| wanted.contains(c)).count();
    let a = count(&[RowClass::Mod, RowClass::Add, RowClass::Img]);
    let d = count(&[RowClass::Mod, RowClass::Del, RowClass::Img]);
    let (green, red) = bar_squares(a, d);
    format!(
        "<span class=\"bar\">{}{}{}</span>",
        "<i class=\"a\"></i>".repeat(green),
        "<i class=\"d\"></i>".repeat(red),
        "<i></i>".repeat(BAR_SQUARES - green - red)
    )
}

fn single_column(blocks: &SideBlocks, status: Status) -> (String, Vec<RowClass>) {
    let (class, glyph, row_class) = match status {
        Status::Add => ("add", "+", RowClass::Add),
        _ => ("del", "−", RowClass::Del),
    };
    let mut html = String::new();
    for i in 0..blocks.len() {
        html.push_str(&format!(
            "<div class=\"row1 {class}\"><div class=\"g\">{glyph}</div><div class=\"c\">{}</div></div>\n",
            blocks.html(i, None)
        ));
    }
    (html, vec![row_class; blocks.len()])
}

/// The title (the old one struck through when it changed).
fn title_html(old: Option<&Page>, new: Option<&Page>) -> String {
    match (old, new) {
        (Some(o), Some(n)) if o.title != n.title => {
            format!("<s>{}</s> {}", esc(&o.title), esc(&n.title))
        }
        (_, Some(p)) | (Some(p), None) => esc(&p.title),
        (None, None) => String::new(),
    }
}

/// The frame (`<details class="file">`) of one document.
pub(super) fn render_frame(ctx: &Ctx, item: &Item, anchor: &str) -> String {
    let base_page = item.base.as_deref().map(|p| &ctx.base.model.pages[p]);
    let head_page = item.head.as_deref().map(|p| &ctx.head.model.pages[p]);
    let base_blocks = || {
        let page = base_page.expect("a base page");
        side_blocks(
            ctx.base,
            ctx,
            item.base.as_deref().unwrap_or(""),
            anchor,
            page,
        )
    };
    let head_blocks = || {
        let page = head_page.expect("a head page");
        side_blocks(
            ctx.head,
            ctx,
            item.head.as_deref().unwrap_or(""),
            anchor,
            page,
        )
    };
    let (body, classes) = match item.status {
        Status::Add => single_column(&head_blocks(), Status::Add),
        Status::Del => single_column(&base_blocks(), Status::Del),
        Status::Mod | Status::Move => {
            let (old, new) = (base_page.expect("base page"), head_page.expect("head page"));
            let mut meta = Vec::new();
            if old.title != new.title {
                meta.push(meta_row("title", &old.title, &new.title));
            }
            let (od, nd) = (
                old.description.as_deref().unwrap_or(""),
                new.description.as_deref().unwrap_or(""),
            );
            if od != nd {
                meta.push(meta_row("description", od, nd));
            }
            let rows = body_rows(&base_blocks(), &head_blocks());
            let classes: Vec<RowClass> = meta.iter().chain(rows.iter()).map(|r| r.class).collect();
            if item.frameless {
                (
                    "<p class=\"nochange\">内容の変更なし</p>\n".to_string(),
                    classes,
                )
            } else {
                let meta_html: String = meta.iter().map(row_html).collect();
                (
                    format!(
                        "<div class=\"split\">\n{meta_html}{}</div>\n",
                        folded_html(&rows)
                    ),
                    classes,
                )
            }
        }
    };
    let path = match (&item.base, &item.head) {
        (Some(b), Some(h)) if b != h => format!("{} → {}", esc(b), esc(h)),
        _ => esc(item.path()),
    };
    let id = item
        .id
        .as_deref()
        .map(|id| format!("<span class=\"id\">{}</span>", esc(id)))
        .unwrap_or_default();
    format!(
        "<details class=\"file\" id=\"{anchor}\" open>\n<summary class=\"fh\"><span class=\"chev\">▾</span>{}<span class=\"ft\">{}</span>{id}<span class=\"fp\">{path}</span>{}</summary>\n{body}</details>\n",
        icon(item.status),
        title_html(base_page, head_page),
        bar_html(&classes),
    )
}
