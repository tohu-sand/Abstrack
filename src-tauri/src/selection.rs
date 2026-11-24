//! Text selection: caret positions from points, the ranges and rectangles a
//! selection covers across pages, and caret movement for the keyboard.

use crate::document::OpenDocument;
use crate::state::{AppState, lock};
use crate::texts::page_text;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use textmodel::{Caret, TextModel, span_on_page};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretPos {
    pub page: usize,
    pub index: usize,
}

impl From<CaretPos> for Caret {
    fn from(c: CaretPos) -> Caret {
        Caret {
            page: c.page,
            index: c.index,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRects {
    pub page: usize,
    /// One rectangle per line, user space [x0, y0, x1, y1].
    pub rects: Vec<[f32; 4]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionInfo {
    pub pages: Vec<PageRects>,
    pub text: String,
}

struct Ctx {
    app: AppHandle,
    doc: Arc<OpenDocument>,
}

impl Ctx {
    fn new(app: &AppHandle, generation: u64) -> Result<Ctx, String> {
        let doc = lock(&app.state::<AppState>().document)
            .clone()
            .ok_or("文書が開かれていません")?;
        if doc.generation != generation {
            return Err(crate::engine::STALE.to_string());
        }
        Ok(Ctx { app: app.clone(), doc })
    }

    fn model(&self, page: usize) -> Result<Arc<TextModel>, String> {
        if page >= self.doc.pages.len() {
            return Err(format!("ページ {} はありません", page + 1));
        }
        let state = self.app.state::<AppState>();
        page_text(&state.engine, &self.doc, page)
    }
}

pub fn caret_at(app: &AppHandle, generation: u64, page: usize, x: f32, y: f32) -> Result<Option<CaretPos>, String> {
    let ctx = Ctx::new(app, generation)?;
    let model = ctx.model(page)?;
    Ok(model.caret_at(x, y).map(|index| CaretPos { page, index }))
}

pub fn selection(app: &AppHandle, generation: u64, anchor: CaretPos, focus: CaretPos) -> Result<SelectionInfo, String> {
    let ctx = Ctx::new(app, generation)?;
    let mut text = String::new();
    let mut pages = Vec::new();
    for page in anchor.page.min(focus.page)..=anchor.page.max(focus.page) {
        let model = ctx.model(page)?;
        let r = span_on_page(anchor.into(), focus.into(), page, || model.page_body());
        if !r.is_empty() {
            pages.push(PageRects { page, rects: model.range_rects(r.start, r.end) });
            model.append_page_text(r, &mut text);
        }
    }
    Ok(SelectionInfo { pages, text })
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Unit {
    Char,
    Word,
    Line,
    Sentence,
}

/// The range of the word or sentence at a caret, for double and triple clicks.
pub fn unit_at(app: &AppHandle, generation: u64, at: CaretPos, unit: Unit) -> Result<(CaretPos, CaretPos), String> {
    let ctx = Ctx::new(app, generation)?;
    let model = ctx.model(at.page)?;
    let (s, e) = match unit {
        Unit::Sentence => model.sentence_at(at.index),
        _ => model.word_at(at.index),
    };
    Ok((CaretPos { page: at.page, index: s }, CaretPos { page: at.page, index: e }))
}

/// Moves a caret by a character, a word or a line. A caret in the body of a
/// page stays in it and goes on into the body of the next (or previous) page
/// with text, past running headers, page numbers and pages without text; a
/// caret placed in a header or footer moves through the whole page. `after`
/// as for [`TextModel::line_step`].
pub fn move_caret(
    app: &AppHandle,
    generation: u64,
    at: CaretPos,
    unit: Unit,
    forward: bool,
    after: bool,
) -> Result<CaretPos, String> {
    let ctx = Ctx::new(app, generation)?;
    let model = ctx.model(at.page)?;
    let body = model.page_body();
    let (lo, hi) = if body.start < body.end && (body.start..=body.end).contains(&at.index) {
        (body.start, body.end)
    } else {
        (0, model.chars.len())
    };
    let at_edge = if forward { at.index >= hi } else { at.index <= lo };
    let within = match unit {
        _ if at_edge => None,
        Unit::Char | Unit::Sentence => Some(if forward { at.index + 1 } else { at.index - 1 }),
        Unit::Word => Some(model.word_boundary(at.index, forward).clamp(lo, hi)),
        Unit::Line => model.line_step(at.index, forward, after).filter(|i| (lo..=hi).contains(i)),
    };
    if let Some(index) = within {
        return Ok(CaretPos { page: at.page, index });
    }
    let offset = model.caret_column(at.index, after).map_or(0.0, |(_, offset)| offset);
    let mut page = at.page;
    loop {
        page = match (forward, page.checked_add_signed(if forward { 1 } else { -1 })) {
            (_, Some(p)) if p < ctx.doc.pages.len() => p,
            _ => return Ok(CaretPos { page: at.page, index: if forward { hi } else { lo } }),
        };
        let next = ctx.model(page)?;
        let b = next.page_body();
        if b.start == b.end {
            continue;
        }
        let index = match unit {
            Unit::Char | Unit::Sentence => if forward { b.start + 1 } else { b.end - 1 },
            Unit::Word if forward => next.word_boundary(b.start, true).min(b.end),
            Unit::Word => next.word_boundary(b.end, false).max(b.start),
            Unit::Line => {
                let line = next.chars[if forward { b.start } else { b.end - 1 }].line;
                next.caret_on_line(line, offset).clamp(b.start, b.end)
            }
        };
        return Ok(CaretPos { page, index });
    }
}
