//! Shapes for annotations made from a selection: QuadPoints for text markup
//! and the place of an insertion mark, as Acrobat makes them. Acrobat's
//! quadrilaterals run from the left edge of the first character to the right
//! edge of the last, from a quarter of an em below the baseline, up to the
//! top the font gives its glyphs (the top of the boxes PDFium reports, but
//! not lower than three quarters of an em above the baseline). Measured on
//! the annotations in `ref/`.

use crate::{Char, TextModel};

const BELOW: f32 = 0.25;
const ABOVE_AT_LEAST: f32 = 0.75;

/// Four corners in user space, in the order of /QuadPoints: top-left,
/// top-right, bottom-left, bottom-right of the text as read.
pub type Corners = [(f32, f32); 4];

/// Where an insertion mark goes: the insertion point and the bottom of the
/// line, in user space, and the height of the line's quadrilateral.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretMark {
    pub x: f32,
    pub bottom: f32,
    pub height: f32,
}

impl TextModel {
    /// One quadrilateral per line for the characters in `start..end`.
    pub fn markup_quads(&self, start: usize, end: usize) -> Vec<Corners> {
        let end = end.min(self.chars.len());
        let mut out = Vec::new();
        let mut from = start;
        while from < end {
            let line = self.chars[from].line;
            let to = (from..end).find(|&i| self.chars[i].line != line).unwrap_or(end);
            let run = &self.chars[from..to];
            let x0 = run.iter().map(|c| c.display.x0).fold(f32::INFINITY, f32::min);
            let x1 = run.iter().map(|c| c.display.x1).fold(f32::NEG_INFINITY, f32::max);
            let (bottom, top) = self.band(run);
            out.push([(x0, top), (x1, top), (x0, bottom), (x1, bottom)].map(|(x, y)| self.frame.from_display(x, y)));
            from = to;
        }
        out
    }

    /// The insertion mark before character `at` (after the last one at the
    /// end of the page). `after` takes a position at a line break to be the
    /// end of the earlier line.
    pub fn caret_mark(&self, at: usize, after: bool) -> Option<CaretMark> {
        let (line, offset) = self.caret_column(at, after)?;
        let chars = &self.chars[self.lines[line].chars.clone()];
        let (bottom, top) = self.band(chars);
        let x = self.blocks[self.lines[line].block].display.x0 + offset;
        let (ux, uy) = self.frame.from_display(x, bottom);
        Some(CaretMark { x: ux, bottom: uy, height: top - bottom })
    }

    /// The bottom and top of Acrobat's quadrilateral for a run of one line,
    /// display space.
    fn band(&self, run: &[Char]) -> (f32, f32) {
        let size = run.iter().map(|c| c.size).fold(0.0f32, f32::max);
        // The anchor of upright text sits a third of an em above the baseline (`segment::anchor`).
        let mut baselines: Vec<f32> = run
            .iter()
            .filter(|c| c.size >= 0.9 * size)
            .map(|c| self.frame.to_display(c.anchor.0, c.anchor.1).1 - 0.33 * c.size)
            .collect();
        baselines.sort_by(f32::total_cmp);
        let baseline = baselines[baselines.len() / 2];
        let top = run
            .iter()
            .filter(|c| c.size >= 0.9 * size)
            .map(|c| c.display.y1)
            .fold(baseline + ABOVE_AT_LEAST * size, f32::max);
        (baseline - BELOW * size, top)
    }
}
