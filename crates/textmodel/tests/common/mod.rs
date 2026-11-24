//! Builds synthetic pages: characters placed in lines like a typesetter would.

#![allow(dead_code)]

use textmodel::{InputChar, MarginBands, PageEdges, PageFrame, Rect, TextModel};

pub struct Page {
    pub chars: Vec<InputChar>,
}

impl Page {
    pub fn new() -> Page {
        Page { chars: Vec::new() }
    }

    /// Places `text` with its baseline at y, starting at x, and returns the x
    /// where it ends. ASCII characters are half an em wide, others one em.
    pub fn put(&mut self, x: f32, y: f32, size: f32, text: &str) -> f32 {
        self.put_raised(x, y, size, 0.0, text)
    }

    /// As `put`, with the baseline raised by `rise` (lowered when negative).
    pub fn put_raised(&mut self, mut x: f32, y: f32, size: f32, rise: f32, text: &str) -> f32 {
        for ch in text.chars() {
            let w = if ch.is_ascii() { size * 0.5 } else { size };
            let base = y + rise;
            self.chars.push(InputChar {
                ch,
                bbox: [x, base - 0.12 * size, x + w, base + 0.88 * size],
                size,
                angle: 0.0,
                source: self.chars.len(),
                origin: Some((x, base)),
                space_before: false,
            });
            x += w;
        }
        x
    }

    pub fn model(&self) -> TextModel {
        TextModel::build(&self.chars, frame())
    }

    pub fn model_with(&self, bands: &MarginBands) -> TextModel {
        TextModel::build_with(&self.chars, frame(), bands)
    }

    pub fn edges(&self) -> PageEdges {
        PageEdges::of(&self.chars, frame())
    }
}

fn frame() -> PageFrame {
    PageFrame::upright(Rect::new(0.0, 0.0, 595.0, 842.0))
}

/// The pages of a document, built with the running headers and footers
/// learnt from all of them.
pub fn document(pages: &[Page]) -> Vec<TextModel> {
    let edges: Vec<PageEdges> = pages.iter().map(Page::edges).collect();
    let bands = MarginBands::learn(&edges);
    pages.iter().map(|p| p.model_with(&bands)).collect()
}

pub fn line_texts(m: &TextModel) -> Vec<String> {
    m.lines.iter().map(|l| m.text(l.chars.clone())).collect()
}

pub const LEFT: f32 = 50.0;
pub const RIGHT: f32 = 310.0;
pub const BODY: f32 = 10.0;

pub fn column(page: &mut Page, x: f32, top: f32, label: char, n: usize) {
    for k in 0..n {
        let text: String = std::iter::repeat_n(label, 23).chain(format!("{k}").chars()).collect();
        page.put(x, top - 15.0 * k as f32, BODY, &text);
    }
}

