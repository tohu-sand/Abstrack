//! Rows of the comment list: one per thread, in reading order, with the text
//! each annotation marks taken from the page's text model.

use crate::state::lock;
use annot::thread::split_edit_text;
use annot::mark::is_marked;
use annot::{Annotation, Kind, Subtype, Thread, latest_state};
use serde::Serialize;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use textmodel::TextModel;

#[derive(Serialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRow {
    pub author: String,
    pub date: String,
    pub text: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommentRow {
    /// Identifies the row while the document stays open (the head annotation).
    pub key: String,
    /// Changes whenever anything shown in the row changes, so that the window
    /// redraws only rows that did. Rows come in reading order; their numbers
    /// (#1, #2, ...) are their places in it.
    pub version: String,
    pub page: usize,
    pub kind: &'static str,
    /// Colour of the mark in the PDF. The window tones it down for the number.
    pub color: Option<String>,
    /// The page text the annotation marks; empty for a point or an area without text.
    pub target: String,
    /// Replacement or insertion text (first line of the Caret's comment).
    pub edit_text: Option<String>,
    pub body: Option<String>,
    pub author: String,
    pub date: String,
    pub timestamp: i64,
    pub replies: Vec<ReplyRow>,
    pub marked: bool,
    pub review: Option<&'static str>,
    /// Bounds of the annotation in user space, for scrolling and the number label.
    pub rect: [f64; 4],
    /// Quadrilaterals to emphasise, user space, 8 numbers each.
    pub quads: Vec<[f64; 8]>,
}

/// A comment's reading position (index in its page's text model) and the
/// text it marks.
type Place = (Option<usize>, String);

/// Places of the comments of the open document, by the shape of what each
/// marks. Page text never changes, so a comment that has not moved is not
/// looked up again, and a change needs the text models of its own pages only.
#[derive(Default)]
pub struct Places {
    generation: u64,
    known: HashMap<u64, Place>,
}

/// The rows of `threads`, checked as `author` has checked them (each person's
/// check marks are their own). `model` gives a page's text model, `None` when it
/// cannot be read; pages are asked for in order, one at a time, and only for
/// comments whose place is not known yet.
pub fn rows(
    annots: &[Annotation],
    threads: &[Thread],
    places: &Mutex<Places>,
    generation: u64,
    author: &str,
    mut model: impl FnMut(usize) -> Option<Arc<TextModel>>,
) -> Vec<CommentRow> {
    let shapes: Vec<u64> = threads.iter().map(|t| shape(annots, t)).collect();
    let mut found: Vec<Option<Place>> = {
        let mut p = lock(places);
        if p.generation != generation {
            *p = Places { generation, known: HashMap::new() };
        }
        shapes.iter().map(|k| p.known.get(k).cloned()).collect()
    };
    let mut missing: Vec<usize> = (0..threads.len()).filter(|&i| found[i].is_none()).collect();
    missing.sort_by_key(|&i| annots[threads[i].head].page);
    let mut current: Option<(usize, Option<Arc<TextModel>>)> = None;
    let mut readable = vec![true; threads.len()];
    for i in missing {
        let page = annots[threads[i].head].page;
        if current.as_ref().map(|c| c.0) != Some(page) {
            current = Some((page, model(page)));
        }
        let m = current.as_ref().and_then(|c| c.1.as_deref());
        readable[i] = m.is_some();
        found[i] = Some(locate(annots, &threads[i], m));
    }
    let found: Vec<Place> = found.into_iter().map(Option::unwrap_or_default).collect();
    {
        let mut p = lock(places);
        if p.generation == generation {
            // Only the comments there are now: places of removed or moved ones go.
            p.known = shapes
                .iter()
                .zip(&found)
                .zip(&readable)
                .filter(|(_, ok)| **ok)
                .map(|((k, place), _)| (*k, place.clone()))
                .collect();
        }
    }
    build_rows(annots, threads, found, author)
}

/// What decides a comment's place: its page, kind and the geometry of what it marks.
fn shape(annots: &[Annotation], t: &Thread) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for a in members(annots, t) {
        (a.page, &a.subtype).hash(&mut h);
        a.rect.iter().for_each(|v| v.to_bits().hash(&mut h));
        a.quads.iter().flat_map(|q| q.points).for_each(|(x, y)| (x.to_bits(), y.to_bits()).hash(&mut h));
    }
    h.finish()
}

struct Placed {
    thread: usize,
    page: usize,
    position: Option<usize>,
    target: String,
}

fn build_rows(annots: &[Annotation], threads: &[Thread], places: Vec<Place>, author: &str) -> Vec<CommentRow> {
    let mut placed: Vec<Placed> = threads
        .iter()
        .zip(places)
        .enumerate()
        .map(|(i, (t, (position, target)))| Placed {
            thread: i,
            page: annots[t.head].page,
            position,
            target,
        })
        .collect();
    placed.sort_by(|a, b| {
        let ra = bounds(annots, &threads[a.thread]);
        let rb = bounds(annots, &threads[b.thread]);
        a.page
            .cmp(&b.page)
            .then(match (a.position, b.position) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => std::cmp::Ordering::Equal,
            })
            .then(rb[3].total_cmp(&ra[3]))
            .then(ra[0].total_cmp(&rb[0]))
    });
    placed
        .into_iter()
        .map(|p| row(annots, &threads[p.thread], p.page, p.target, author))
        .collect()
}

fn members<'a>(annots: &'a [Annotation], t: &'a Thread) -> impl Iterator<Item = &'a Annotation> {
    std::iter::once(t.head).chain(t.members.iter().copied()).map(move |i| &annots[i])
}

fn bounds(annots: &[Annotation], t: &Thread) -> [f64; 4] {
    members(annots, t).fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, a| {
        [b[0].min(a.rect[0]), b[1].min(a.rect[1]), b[2].max(a.rect[2]), b[3].max(a.rect[3])]
    })
}

/// Reading position (index in the page's text model) and the marked text.
fn locate(annots: &[Annotation], t: &Thread, model: Option<&TextModel>) -> (Option<usize>, String) {
    let Some(model) = model.filter(|m| !m.is_empty()) else {
        return (None, String::new());
    };
    let quads: Vec<[(f32, f32); 4]> = members(annots, t)
        .flat_map(|a| a.quads.iter())
        .map(|q| q.points.map(|(x, y)| (x as f32, y as f32)))
        .collect();
    let head = &annots[t.head];
    let r = head.rect.map(|v| v as f32);
    if !quads.is_empty() {
        let hits = model.chars_in_quads(&quads);
        if let Some(&first) = hits.first() {
            return (Some(first), model.text_of(&hits));
        }
        let (x, y) = quads[0][2];
        return (model.insertion_point(x, y), String::new());
    }
    match head.subtype {
        Subtype::Caret => (model.insertion_point((r[0] + r[2]) / 2.0, r[1]), String::new()),
        Subtype::Square | Subtype::Circle | Subtype::Polygon | Subtype::Ink => {
            let inside: Vec<usize> = model
                .chars
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let (x, y) = c.anchor;
                    x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]
                })
                .map(|(i, _)| i)
                .collect();
            match inside.first() {
                Some(&first) => (Some(first), model.text_of(&inside)),
                None => (model.nearest_char(r[0], r[3]), String::new()),
            }
        }
        _ => (model.nearest_char((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0), String::new()),
    }
}

fn row(annots: &[Annotation], t: &Thread, page: usize, target: String, me: &str) -> CommentRow {
    let head = &annots[t.head];
    let caret = members(annots, t).find(|a| a.subtype == Subtype::Caret);
    let (edit_text, body) = match (t.kind, caret.and_then(|c| c.contents.as_deref())) {
        (Kind::Replace | Kind::Insert, Some(text)) => {
            let (edit, note) = split_edit_text(text);
            (Some(edit.to_string()), note.map(str::to_string))
        }
        (Kind::Replace | Kind::Insert, None) => (None, None),
        _ => (None, head.contents.clone().filter(|c| !c.is_empty())),
    };
    let date = head.created.or(head.modified);
    // The strike-out covers the original text; a grouped caret can be in the margin.
    let color_of = |a: &Annotation| annot::model::color_rgb(&a.color).map(crate::settings::to_hex);
    let color = members(annots, t)
        .filter(|a| t.kind == Kind::Replace && a.subtype == Subtype::StrikeOut)
        .find_map(color_of)
        .or_else(|| members(annots, t).find_map(color_of));
    let quads = members(annots, t)
        .flat_map(|a| a.quads.iter())
        .map(|q| {
            let p = q.points;
            [p[0].0, p[0].1, p[1].0, p[1].1, p[2].0, p[2].1, p[3].0, p[3].1]
        })
        .collect();
    let mut row = CommentRow {
        key: head.key.to_string(),
        version: String::new(),
        page,
        kind: t.kind.label(),
        color,
        target,
        edit_text,
        body,
        author: head.author.clone().unwrap_or_default(),
        date: date.map(|d| d.to_string()).unwrap_or_default(),
        timestamp: date.map(|d| d.timestamp()).unwrap_or(0),
        replies: t
            .replies
            .iter()
            .map(|&i| {
                let a = &annots[i];
                ReplyRow {
                    author: a.author.clone().unwrap_or_default(),
                    date: a.created.or(a.modified).map(|d| d.to_string()).unwrap_or_default(),
                    text: a.contents.clone().unwrap_or_default(),
                }
            })
            .collect(),
        marked: is_marked(annots, t, me),
        review: latest_state(annots, t, "Review").and_then(|s| review_label(s.state.as_deref())),
        rect: bounds(annots, t),
        quads,
    };
    row.version = version(&row);
    row
}

fn version(row: &CommentRow) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    row.color.hash(&mut h);
    (row.page, row.kind, &row.target, &row.edit_text, &row.body, &row.author, &row.date, row.timestamp).hash(&mut h);
    (&row.replies, row.marked, row.review).hash(&mut h);
    row.rect.iter().chain(row.quads.iter().flatten()).for_each(|v| v.to_bits().hash(&mut h));
    format!("{:x}", h.finish())
}

fn review_label(state: Option<&str>) -> Option<&'static str> {
    match state? {
        "Accepted" => Some("承認"),
        "Rejected" => Some("却下"),
        "Cancelled" => Some("キャンセル"),
        "Completed" => Some("完了"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annot::write::{self, Colors, Context, NewComment};
    use annot::{PdfDate, build_threads, read_all};
    use pdf_io::{Object, PdfFile};
    use textmodel::{PageFrame, Rect};

    /// The fixture with a note at each (page, height).
    fn noted(notes: &[(usize, f64)]) -> (Vec<Annotation>, Vec<Thread>) {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/abstract-2col.pdf")).unwrap();
        let mut file = PdfFile::open(bytes).unwrap();
        let mut next = file.doc.trailer.get(b"Size").and_then(Object::as_i64).unwrap() as u32;
        let now = PdfDate { year: 2026, month: 11, day: 22, hour: 9, minute: 0, second: 0, offset_minutes: Some(540) };
        for &(page, y) in notes {
            let page_id = file.page_ids()[page];
            let ctx = Context { page: page_id, page_right: 595.0, author: "a", now, colors: Colors::default() };
            let built = write::build(&NewComment::Note { at: (60.0, y) }, "x", &ctx, &mut || {
                next += 1;
                (next - 1, 0)
            });
            file.doc.objects.extend(built.objects);
            let (id, annots) = write::change_annots(&file.doc, page_id, &built.annots, true).unwrap();
            file.doc.objects.insert(id, annots);
        }
        let annots = read_all(&file);
        let threads = build_threads(&annots);
        (annots, threads)
    }

    /// Asks for page text and counts the pages asked for.
    fn build(annots: &[Annotation], threads: &[Thread], places: &Mutex<Places>, asked: &mut Vec<usize>) -> Vec<CommentRow> {
        rows(annots, threads, places, 1, "a", |page| {
            asked.push(page);
            Some(Arc::new(TextModel::build(&[], PageFrame::upright(Rect::new(0.0, 0.0, 595.0, 842.0)))))
        })
    }

    #[test]
    fn pages_are_read_again_only_for_comments_that_moved() {
        let places = Mutex::new(Places::default());
        let (annots, threads) = noted(&[(0, 700.0), (0, 600.0), (1, 700.0)]);
        let mut asked = Vec::new();
        assert_eq!(build(&annots, &threads, &places, &mut asked).len(), 3);
        assert_eq!(asked, vec![0, 1], "one page at a time, in order");

        asked.clear();
        build(&annots, &threads, &places, &mut asked);
        assert!(asked.is_empty(), "nothing moved: {asked:?}");

        let (annots, threads) = noted(&[(0, 700.0), (0, 600.0), (1, 650.0)]);
        asked.clear();
        assert_eq!(build(&annots, &threads, &places, &mut asked).len(), 3);
        assert_eq!(asked, vec![1]);
    }

    #[test]
    fn row_number_uses_the_annotations_own_color() {
        let (mut annots, threads) = noted(&[(0, 700.0)]);
        let head = threads[0].head;
        let places = Mutex::new(Places::default());
        let mut versions = std::collections::HashSet::new();
        for (components, expected) in [
            (vec![0.2, 0.6, 0.4], Some("#339966")),
            (vec![0.5], Some("#808080")),
            (vec![1.0, 0.0, 0.0, 0.0], Some("#00ffff")),
            (vec![], None),
        ] {
            annots[head].color = components;
            let rows = build(&annots, &threads, &places, &mut Vec::new());
            assert_eq!(rows[0].color.as_deref(), expected);
            assert!(versions.insert(rows[0].version.clone()));
        }
    }

    #[test]
    fn a_page_that_could_not_be_read_is_asked_for_again() {
        let places = Mutex::new(Places::default());
        let (annots, threads) = noted(&[(0, 700.0)]);
        let mut asked = 0;
        rows(&annots, &threads, &places, 1, "a", |_| {
            asked += 1;
            None
        });
        let mut again = Vec::new();
        build(&annots, &threads, &places, &mut again);
        assert_eq!((asked, again), (1, vec![0]));
    }
}
