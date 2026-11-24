//! The text of a page in reading order, built from character boxes.
//!
//! Input is what PDFium reports per character; output is the characters
//! ordered by blocks (column regions), lines and position. Positions in this
//! order are what a selection or an annotation's range refers to.

mod extract;
mod geometry;
mod layout;
mod locate;
mod margins;
mod quads;
mod search;
mod segment;
mod select;
mod stacked;

pub use geometry::{PageFrame, Rect};
pub use margins::{MarginBands, PageEdges, sample_pages};
pub use quads::{CaretMark, Corners};
pub use search::{Match, SearchQuery, SearchText};
pub use select::{Caret, PageBody, joins_with_space, span, span_on_page, span_text};

use std::ops::Range;

#[derive(Debug, Clone)]
pub struct InputChar {
    pub ch: char,
    /// [x0, y0, x1, y1] in user space.
    pub bbox: [f32; 4],
    pub size: f32,
    /// Counter-clockwise, in degrees, in user space.
    pub angle: f32,
    /// Start of the glyph on its baseline, user space, when known.
    pub origin: Option<(f32, f32)>,
    /// Position in the order the source reported the characters.
    pub source: usize,
    /// The source put a word break (a space it inferred, not a glyph) right before this character.
    pub space_before: bool,
}

/// A character as a text extractor reports it, including the spaces and line
/// breaks it inferred itself (`generated`).
#[derive(Debug, Clone)]
pub struct SourceChar {
    pub ch: char,
    pub bbox: [f32; 4],
    pub size: f32,
    pub angle: f32,
    pub origin: (f32, f32),
    pub index: usize,
    pub generated: bool,
}

/// Drops inferred characters, keeping an inferred space as a word break on the next character.
pub fn inputs_from_source<C: std::borrow::Borrow<SourceChar>>(chars: impl IntoIterator<Item = C>) -> Vec<InputChar> {
    let chars = chars.into_iter();
    let mut out = Vec::with_capacity(chars.size_hint().0);
    let mut pending_space = false;
    for c in chars {
        let c = c.borrow();
        if c.generated {
            if c.ch == ' ' {
                pending_space = true;
            }
            continue;
        }
        out.push(InputChar {
            ch: c.ch,
            bbox: c.bbox,
            size: c.size,
            angle: c.angle,
            origin: Some(c.origin),
            source: c.index,
            space_before: pending_space,
        });
        pending_space = false;
    }
    out
}

#[derive(Debug, Clone)]
pub struct Char {
    pub ch: char,
    /// Box in user space.
    pub rect: Rect,
    /// Box in display space (see [`PageFrame::to_display`]).
    pub display: Rect,
    /// A point in the middle of the glyph's ink, user space. Math fonts report
    /// boxes much taller than their glyphs, so the box centre can fall outside
    /// the area a markup annotation covers; this point does not.
    pub anchor: (f32, f32),
    pub size: f32,
    pub source: usize,
    pub space_before: bool,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub chars: Range<usize>,
    /// Union of the character boxes, display space.
    pub display: Rect,
    pub block: usize,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub lines: Range<usize>,
    pub display: Rect,
    pub role: Role,
}

/// Whether a block is running text or sits in the top or bottom margin
/// (running headers, page numbers, footers). Margin blocks come first and
/// last in reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Header,
    Body,
    Footer,
}

#[derive(Debug, Clone)]
pub struct TextModel {
    pub frame: PageFrame,
    pub chars: Vec<Char>,
    pub lines: Vec<Line>,
    pub blocks: Vec<Block>,
}

impl TextModel {
    /// The model of a page, with page numbers as its only margin rows.
    pub fn build(input: &[InputChar], frame: PageFrame) -> TextModel {
        TextModel::build_with(input, frame, &MarginBands::default())
    }

    /// The model of a page of a document whose running headers and footers
    /// sit in `bands`.
    pub fn build_with(input: &[InputChar], frame: PageFrame, bands: &MarginBands) -> TextModel {
        let glyphs = segment::glyphs(input, &frame);
        let segments = segment::segments(&glyphs);
        let body = layout::body_size(&segments);
        let page = frame.rect_to_display(&frame.bbox);
        let margins = margins::find(&glyphs, &segments, &page, body, bands);
        let groups = layout::order(&segments, body, margins);

        let mut chars = Vec::with_capacity(glyphs.len());
        let mut lines = Vec::new();
        let mut blocks = Vec::new();
        for group in groups {
            let first_line = lines.len();
            let mut block_rect: Option<Rect> = None;
            for line in group.lines {
                let stacked = |s: usize| line.stacked.iter().any(|&(_, part)| part == s);
                let by_position = |&a: &usize, &b: &usize| {
                    let (ra, rb) = (&glyphs[a].display, &glyphs[b].display);
                    ra.x0.total_cmp(&rb.x0).then(rb.y1.total_cmp(&ra.y1))
                };
                let mut members: Vec<usize> = line
                    .segments
                    .iter()
                    .filter(|&&s| !stacked(s))
                    .flat_map(|&s| segments[s].glyphs.clone())
                    .collect();
                members.sort_by(by_position);
                // A part stacked on another (a denominator under its
                // numerator) follows the last character of that part.
                let mut hosts: Vec<usize> = line.stacked.iter().map(|&(host, _)| host).collect();
                hosts.dedup();
                for host in hosts {
                    let mut parts: Vec<usize> = line
                        .stacked
                        .iter()
                        .filter(|&&(h, _)| h == host)
                        .flat_map(|&(_, part)| segments[part].glyphs.clone())
                        .collect();
                    parts.sort_by(by_position);
                    let after = members
                        .iter()
                        .rposition(|g| segments[host].glyphs.contains(g))
                        .map_or(members.len(), |p| p + 1);
                    members.splice(after..after, parts);
                }
                if members.is_empty() {
                    continue;
                }
                let start = chars.len();
                let mut line_rect = glyphs[members[0]].display;
                for g in members {
                    let g = &glyphs[g];
                    line_rect = line_rect.union(&g.display);
                    chars.push(Char {
                        ch: g.ch,
                        rect: g.user,
                        display: g.display,
                        anchor: g.anchor,
                        size: g.size,
                        source: g.source,
                        space_before: g.space_before,
                        line: lines.len(),
                    });
                }
                block_rect = Some(block_rect.map_or(line_rect, |b| b.union(&line_rect)));
                lines.push(Line {
                    chars: start..chars.len(),
                    display: line_rect,
                    block: blocks.len(),
                });
            }
            if let Some(display) = block_rect {
                blocks.push(Block {
                    lines: first_line..lines.len(),
                    display,
                    role: group.role,
                });
            }
        }
        TextModel {
            frame,
            chars,
            lines,
            blocks,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}
