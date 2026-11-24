//! Drags over real pages: from a point at the start of one piece of text to a
//! point at the end of another, the selected string is what a reader expects.
//! The selection is worked out as the application does: carets at the two
//! points, the range between them across pages, page bodies without running
//! headers and page numbers (learnt from the document's pages).

use render::{ChainedBytes, Document, Engine};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use textmodel::{Caret, MarginBands, PageEdges, TextModel, sample_pages, span, span_text};

#[derive(Debug, Deserialize)]
pub struct DragCase {
    pub file: String,
    pub category: String,
    pub from: Anchor,
    pub to: Anchor,
    /// Compared without white space: where spaces go is not what is tested.
    pub expect: String,
}

/// A piece of text on a page. The drag starts just inside the left edge of
/// its first character (`from`) or ends just inside the right edge of its
/// last (`to`).
#[derive(Debug, Deserialize)]
pub struct Anchor {
    /// From 1.
    pub page: usize,
    /// Matched against the page text without white space.
    pub text: String,
    /// Which occurrence, from 0.
    #[serde(default)]
    pub nth: usize,
    /// [offset, length] of the characters to use within the occurrence.
    #[serde(default)]
    pub part: Option<[usize; 2]>,
    /// Moves the point, in points of the page as shown (x right, y up), to
    /// drag from or to the space beside the text.
    #[serde(default)]
    pub dx: f32,
    #[serde(default)]
    pub dy: f32,
}

struct Pages<'a> {
    doc: Document<'a>,
    bands: MarginBands,
    models: HashMap<usize, TextModel>,
}

impl Pages<'_> {
    fn model(&mut self, page: usize) -> &TextModel {
        let (doc, bands) = (&self.doc, &self.bands);
        self.models.entry(page).or_insert_with(|| {
            let (input, frame) = super::page_input(doc, page);
            TextModel::build_with(&input, frame, bands)
        })
    }
}

/// Runs the cases; returns a description of each that failed.
pub fn check(engine: &Engine, dir: &Path, cases: &[DragCase]) -> Vec<String> {
    let mut docs: HashMap<String, Pages<'_>> = HashMap::new();
    let mut failures = Vec::new();
    for case in cases {
        let pages = docs.entry(case.file.clone()).or_insert_with(|| {
            let bytes = std::fs::read(dir.join(&case.file)).unwrap_or_else(|e| panic!("{}: {e}", case.file));
            let doc = engine.open(ChainedBytes::new(vec![Arc::new(bytes)])).unwrap();
            let edges: Vec<PageEdges> = sample_pages(doc.page_count())
                .into_iter()
                .map(|p| {
                    let (input, frame) = super::page_input(&doc, p);
                    PageEdges::of(&input, frame)
                })
                .collect();
            Pages { bands: MarginBands::learn(&edges), doc, models: HashMap::new() }
        });
        match select(pages, case) {
            Ok(text) if strip(&text) == strip(&case.expect) => {}
            Ok(text) => failures.push(format!(
                "{} [{}]: expected {:?}\n      selected {:?}",
                case.file, case.category, case.expect, text
            )),
            Err(e) => failures.push(format!("{} [{}]: {e}", case.file, case.category)),
        }
    }
    failures
}

fn select(pages: &mut Pages<'_>, case: &DragCase) -> Result<String, String> {
    let a = caret(pages, &case.from, true)?;
    let b = caret(pages, &case.to, false)?;
    let (low, high) = (a.page.min(b.page), a.page.max(b.page));
    for p in low..=high {
        pages.model(p);
    }
    let models = &pages.models;
    let ranges = span(a, b, |p| models[&p].page_body());
    Ok(span_text(&ranges, |p| &models[&p]))
}

fn caret(pages: &mut Pages<'_>, anchor: &Anchor, start: bool) -> Result<Caret, String> {
    let page = anchor.page - 1;
    let m = pages.model(page);
    let chars = find(m, anchor).ok_or_else(|| format!("{:?} not found on page {}", anchor.text, anchor.page))?;
    let c = &m.chars[if start { chars[0] } else { chars[chars.len() - 1] }];
    let (_, y) = m.frame.to_display(c.anchor.0, c.anchor.1);
    let x = if start { c.display.x0 + 0.5 } else { c.display.x1 - 0.5 };
    let (ux, uy) = m.frame.from_display(x + anchor.dx, y + anchor.dy);
    let index = m.caret_at(ux, uy).ok_or("no text on the page")?;
    Ok(Caret { page, index })
}

fn find(m: &TextModel, anchor: &Anchor) -> Option<Vec<usize>> {
    let all: Vec<usize> = (0..m.chars.len()).collect();
    let (text, sources) = m.text_with_sources(&all);
    let hay: Vec<(char, usize)> = text
        .chars()
        .zip(sources)
        .filter_map(|(ch, s)| s.filter(|_| !ch.is_whitespace()).map(|s| (ch, s)))
        .collect();
    let needle: Vec<char> = anchor.text.chars().filter(|c| !c.is_whitespace()).collect();
    if needle.is_empty() {
        return None;
    }
    let start = (0..hay.len().saturating_sub(needle.len() - 1))
        .filter(|&i| hay[i..i + needle.len()].iter().map(|h| h.0).eq(needle.iter().copied()))
        .nth(anchor.nth)?;
    let [offset, len] = anchor.part.unwrap_or([0, needle.len()]);
    Some(hay[start + offset..start + offset + len].iter().map(|h| h.1).collect())
}

fn strip(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
