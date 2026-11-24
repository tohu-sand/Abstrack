//! What a text comment marks, as a range of its page's reading order, and
//! moving the ends of that range with the handles on the page. The comment's
//! annotations are rewritten in place, not made again.

use crate::annotate::{self, Range};
use crate::clock;
use crate::selection::CaretPos;
use crate::state::{AppState, lock};
use annot::{Annotation, Kind, Subtype, Thread};
use serde::Serialize;
use std::cmp::Ordering;
use tauri::{AppHandle, Manager};
use textmodel::TextModel;

/// The caret before the first character a comment marks, and the one after
/// its last.
#[derive(Serialize)]
pub struct Marked {
    pub start: CaretPos,
    pub end: CaretPos,
}

/// The text markup whose range moves, and for a replacement the insertion
/// mark that stands after the struck-out text.
struct Parts {
    markup: usize,
    mark: Option<usize>,
}

fn parts(annots: &[Annotation], t: &Thread) -> Option<Parts> {
    let parts = match (t.kind, t.members.as_slice()) {
        (Kind::Highlight | Kind::Delete | Kind::Underline, []) => Parts { markup: t.head, mark: None },
        (Kind::Replace, &[strike]) if annots[t.head].subtype == Subtype::Caret => Parts { markup: strike, mark: Some(t.head) },
        _ => return None,
    };
    let markup = &annots[parts.markup];
    let movable = matches!(markup.subtype, Subtype::Highlight | Subtype::StrikeOut | Subtype::Underline);
    (movable && !markup.quads.is_empty()).then_some(parts)
}

/// The range `key`'s comment marks, when its ends can be moved: one text
/// markup over characters that follow one another in reading order (a
/// marking made of separate stretches has no handles).
pub fn marked(app: &AppHandle, generation: u64, key: &str) -> Result<Option<Marked>, String> {
    let doc = lock(&app.state::<AppState>().document).clone().ok_or("文書が開かれていません")?;
    if doc.generation != generation {
        return Err(crate::engine::STALE.to_string());
    }
    if doc.read_only.is_some() {
        return Ok(None);
    }
    let (page, quads) = {
        let edit = doc.edit();
        // A comment removed meanwhile has no handles.
        let Ok((annots, thread)) = annotate::find(&edit.annots, &edit.threads, key) else { return Ok(None) };
        let Some(p) = parts(annots, thread) else { return Ok(None) };
        let markup = &annots[p.markup];
        if annotate::object_id(markup).is_err() {
            return Ok(None);
        }
        let quads: Vec<[(f32, f32); 4]> = markup.quads.iter().map(|q| q.points.map(|(x, y)| (x as f32, y as f32))).collect();
        (markup.page, quads)
    };
    let model = annotate::model(app, &doc, page)?;
    let hits = model.chars_in_quads(&quads);
    let (Some(&first), Some(&last)) = (hits.first(), hits.last()) else { return Ok(None) };
    // Acrobat's quadrilaterals and this program's characters may disagree on
    // a character or two; more left out means separate stretches.
    let skipped = last + 1 - first - hits.len();
    if skipped > 2.max((last + 1 - first) / 20) {
        return Ok(None);
    }
    Ok(Some(Marked {
        start: CaretPos { page, index: first },
        end: CaretPos { page, index: last + 1 },
    }))
}

/// Makes `key`'s comment mark `range`, kept on the comment's page: the text
/// markup takes new quadrilaterals, and the insertion mark of a replacement
/// moves to the new end. One change, undone as a whole.
pub fn reshape(app: &AppHandle, generation: u64, key: &str, range: Range) -> Result<(), String> {
    let doc = annotate::document(app, generation)?;
    let (page, markup_id, mark_id) = {
        let edit = doc.edit();
        let (annots, thread) = annotate::find(&edit.annots, &edit.threads, key)?;
        let p = parts(annots, thread).ok_or("このコメントは範囲を変えられません")?;
        let mark = p.mark.map(|i| annotate::object_id(&annots[i])).transpose()?;
        (annots[p.markup].page, annotate::object_id(&annots[p.markup])?, mark)
    };
    let model = annotate::model(app, &doc, page)?;
    let (start, end) = on_page(range, page, &model);
    if start >= end {
        return Err("範囲に文字がありません。コメントを消すときは Delete キーを使ってください".into());
    }
    let quads: Vec<[(f64, f64); 4]> = model
        .markup_quads(start, end)
        .into_iter()
        .map(|c| c.map(|(x, y)| (x as f64, y as f64)))
        .collect();
    let caret = mark_id.map(|_| annotate::mark(&model, end, true)).transpose()?;
    let now = clock::now();
    let mut edit = doc.edit();
    let dict = |edit: &crate::edit::Edit, id| edit.file.doc.get_dictionary(id).cloned().map_err(|e| e.to_string());
    let markup = dict(&edit, markup_id)?;
    let mut objects = annot::reshape::markup(markup_id, &markup, &quads, now, &mut || edit.new_id())?;
    if let (Some(id), Some(caret)) = (mark_id, caret) {
        let mark = dict(&edit, id)?;
        objects.extend(annot::reshape::caret(id, &mark, caret, now, &mut || edit.new_id()));
    }
    edit.apply("範囲の変更", objects.into_iter().map(|(id, o)| (id, Some(o))).collect());
    drop(edit);
    annotate::publish(app, &doc)
}

/// The part of `range` on `page`, sorted: an end on an earlier page counts as
/// the start of the page's body, one on a later page as its end.
fn on_page(range: Range, page: usize, model: &TextModel) -> (usize, usize) {
    let body = model.page_body();
    let place = |c: CaretPos| match c.page.cmp(&page) {
        Ordering::Less => body.start,
        Ordering::Greater => body.end,
        Ordering::Equal => c.index.min(body.len),
    };
    let (a, b) = (place(range.anchor), place(range.focus));
    (a.min(b), a.max(b))
}
