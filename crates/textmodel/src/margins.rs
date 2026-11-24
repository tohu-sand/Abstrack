//! Running headers, page numbers and footers: rows of text at the top or
//! bottom edge of a page that are not part of the running text. They are
//! taken out before the columns are worked out and read before (headers) or
//! after (footers) everything else, so that a selection through the running
//! text never takes them in, wherever on the row they sit.
//!
//! A row counts when it is a page number, or when it sits at a height where
//! the document has running headers or footers, learnt beforehand from a few
//! of its pages (see [`MarginBands::learn`]). The look of one page is not
//! enough: a figure caption at the foot of a page or the first line under a
//! section break can be set apart just like a running header.

use crate::InputChar;
use crate::geometry::{PageFrame, Rect};
use crate::segment::{self, Glyph, Segment};

/// How far into the page, as a fraction of its height, a margin row may reach.
const ZONE: f32 = 1.0 / 8.0;
/// Rows considered at each edge.
const MAX_ROWS: usize = 2;
/// How far apart, in points, the same running header may sit on two pages.
const TOLERANCE: f32 = 2.0;

/// The rows nearest the top and bottom edges of one page.
#[derive(Debug, Clone, Default)]
pub struct PageEdges {
    rows: Vec<EdgeRow>,
}

#[derive(Debug, Clone)]
struct EdgeRow {
    top: bool,
    y0: f32,
    y1: f32,
    /// The wording without digits and white space, so that rows differing
    /// only in the page number match.
    key: String,
    /// Set well apart from the next row towards the middle of the page.
    isolated: bool,
    /// The largest type size on the row.
    size: f32,
    /// Not larger than the running text of its page, as running heads are.
    small: bool,
}

/// Heights at which a document has running headers (top) or footers.
#[derive(Debug, Clone, Default)]
pub struct MarginBands {
    bands: Vec<Band>,
}

#[derive(Debug, Clone, Copy)]
struct Band {
    top: bool,
    y0: f32,
    y1: f32,
    /// The largest type size seen in the band, so that a title set at the
    /// same height on the first page is not taken for a running header.
    size: f32,
}

impl PageEdges {
    pub fn of(input: &[InputChar], frame: PageFrame) -> PageEdges {
        let glyphs = segment::glyphs(input, &frame);
        let segments = segment::segments(&glyphs);
        let page = frame.rect_to_display(&frame.bbox);
        let body = crate::layout::body_size(&segments);
        let rows = rows(&segments);
        let mut out = Vec::new();
        for top in [true, false] {
            let from_edge: Vec<&Row> = if top { rows.iter().collect() } else { rows.iter().rev().collect() };
            for (k, row) in from_edge.iter().enumerate().take(MAX_ROWS) {
                if !in_zone(row, &page, top) {
                    break;
                }
                let isolated = from_edge.get(k + 1).is_some_and(|next| {
                    let gap = if top { row.y0 - next.y1 } else { next.y0 - row.y1 };
                    gap >= (2.0 * (row.y1 - row.y0)).max(1.5 * body)
                });
                out.push(EdgeRow {
                    top,
                    y0: row.y0,
                    y1: row.y1,
                    key: key(&text(row, &segments, &glyphs)),
                    isolated,
                    size: largest(row, &segments),
                    small: largest(row, &segments) <= body * 1.2,
                });
            }
        }
        PageEdges { rows: out }
    }
}

impl MarginBands {
    /// Heights at which the sample pages have running headers or footers:
    /// where at least two in five of them have a row with the same wording
    /// (running heads that alternate between left and right pages are still
    /// on half of them), or where half of them have a row set well apart
    /// from the text (running heads that change with the chapter). A label
    /// that repeats on a page or two at the same height does not count.
    pub fn learn(pages: &[PageEdges]) -> MarginBands {
        let n = pages.len();
        let same_wording = (n * 2).div_ceil(5).max(2);
        let set_apart = n.div_ceil(2).max(2);
        let centre = |r: &EdgeRow| (r.y0 + r.y1) / 2.0;
        let pages_with = |row: &EdgeRow, also: &dyn Fn(&EdgeRow) -> bool| {
            pages
                .iter()
                .filter(|p| {
                    p.rows
                        .iter()
                        .any(|r| r.top == row.top && (centre(r) - centre(row)).abs() <= TOLERANCE && also(r))
                })
                .count()
        };
        let mut bands: Vec<Band> = Vec::new();
        for row in pages.iter().flat_map(|p| &p.rows).filter(|r| r.small) {
            let repeated = worded(&row.key) && pages_with(row, &|r| r.key == row.key) >= same_wording;
            let apart = row.isolated && pages_with(row, &|r| r.isolated) >= set_apart;
            if repeated || apart {
                add(&mut bands, Band { top: row.top, y0: row.y0, y1: row.y1, size: row.size });
            }
        }
        MarginBands { bands }
    }

    fn holds(&self, top: bool, row: &Row, size: f32) -> bool {
        let c = (row.y0 + row.y1) / 2.0;
        self.bands.iter().any(|b| {
            b.top == top && c >= b.y0 - TOLERANCE && c <= b.y1 + TOLERANCE && size <= b.size * 1.1 + 0.5
        })
    }
}

fn add(bands: &mut Vec<Band>, band: Band) {
    match bands
        .iter_mut()
        .find(|b| b.top == band.top && b.y0 <= band.y1 + TOLERANCE && band.y0 <= b.y1 + TOLERANCE)
    {
        Some(b) => {
            b.y0 = b.y0.min(band.y0);
            b.y1 = b.y1.max(band.y1);
            b.size = b.size.max(band.size);
        }
        None => bands.push(band),
    }
}

/// Pages to read for [`MarginBands::learn`]: all of a short document,
/// otherwise six pairs of facing pages spread over it, so that headers that
/// alternate between left and right pages are seen too.
pub fn sample_pages(count: usize) -> Vec<usize> {
    const PAIRS: usize = 6;
    if count <= 2 * PAIRS {
        return (0..count).collect();
    }
    let mut out: Vec<usize> = (0..PAIRS)
        .flat_map(|k| {
            let p = 1 + k * (count - 3) / (PAIRS - 1);
            [p, p + 1]
        })
        .collect();
    out.dedup();
    out
}

#[derive(Debug, Default)]
pub(crate) struct Margins {
    pub header: Vec<usize>,
    pub footer: Vec<usize>,
}

/// The segments of a page that form its running header and footer.
pub(crate) fn find(glyphs: &[Glyph], segments: &[Segment], page: &Rect, body: f32, bands: &MarginBands) -> Margins {
    let rows = rows(segments);
    let marginal = |row: &Row, top: bool| {
        let size = largest(row, segments);
        in_zone(row, page, top)
            && (bands.holds(top, row, size) || (size <= body * 1.2 && page_number(&text(row, segments, glyphs))))
    };
    let from_top: Vec<&Row> = rows.iter().collect();
    let top = count(&from_top, segments, body, true, |r| marginal(r, true));
    let from_bottom: Vec<&Row> = rows[top..].iter().rev().collect();
    let bottom = count(&from_bottom, segments, body, false, |r| marginal(r, false));
    let members = |rows: &[Row]| -> Vec<usize> { rows.iter().flat_map(|r| r.members.iter().copied()).collect() };
    Margins {
        header: members(&rows[..top]),
        footer: members(&rows[rows.len() - bottom..]),
    }
}

/// How many rows from the edge are margin rows. They must keep clear of the
/// text further in that lies above or below them (a tall bracket of an
/// equation beside a page number does not count).
fn count(rows: &[&Row], segments: &[Segment], body: f32, top: bool, marginal: impl Fn(&Row) -> bool) -> usize {
    let mut k = 0;
    while k < MAX_ROWS.min(rows.len()) && marginal(rows[k]) {
        k += 1;
    }
    let clear = |k: usize| {
        rows[..k].iter().flat_map(|r| &r.members).all(|&m| {
            let m = &segments[m].rect;
            rows[k..].iter().flat_map(|r| &r.members).all(|&s| {
                let s = &segments[s].rect;
                let gap = if top { m.y0 - s.y1 } else { s.y0 - m.y1 };
                m.horizontal_overlap(s) <= 0.0 || gap >= 0.3 * body
            })
        })
    };
    while k > 0 && !clear(k) {
        k -= 1;
    }
    k
}

fn largest(row: &Row, segments: &[Segment]) -> f32 {
    row.members.iter().map(|&i| segments[i].size).fold(0.0f32, f32::max)
}

/// Upright segments whose vertical extents overlap, top to bottom.
struct Row {
    y0: f32,
    y1: f32,
    members: Vec<usize>,
}

fn rows(segments: &[Segment]) -> Vec<Row> {
    let mut ids: Vec<usize> = (0..segments.len()).filter(|&i| segments[i].upright).collect();
    ids.sort_by(|&a, &b| segments[b].rect.y1.total_cmp(&segments[a].rect.y1));
    let mut rows: Vec<Row> = Vec::new();
    for i in ids {
        let r = &segments[i].rect;
        match rows.last_mut() {
            Some(row) if r.center().1 >= row.y0 => {
                row.y0 = row.y0.min(r.y0);
                row.members.push(i);
            }
            _ => rows.push(Row {
                y0: r.y0,
                y1: r.y1,
                members: vec![i],
            }),
        }
    }
    rows
}

fn in_zone(row: &Row, page: &Rect, top: bool) -> bool {
    if top {
        row.y0 >= page.y1 - ZONE * page.height()
    } else {
        row.y1 <= page.y0 + ZONE * page.height()
    }
}

fn text(row: &Row, segments: &[Segment], glyphs: &[Glyph]) -> String {
    let mut members = row.members.clone();
    members.sort_by(|&a, &b| segments[a].rect.x0.total_cmp(&segments[b].rect.x0));
    members
        .iter()
        .flat_map(|&s| glyphs[segments[s].glyphs.clone()].iter().map(|g| g.ch))
        .collect()
}

fn key(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace() && !c.is_numeric()).collect()
}

/// Enough wording to tell a running header from a stray label or an
/// equation number that happens to sit at the same height.
fn worded(key: &str) -> bool {
    key.chars().count() >= 3 && key.chars().filter(|c| c.is_alphabetic()).count() >= 2
}

/// "12", "- 12 -", "12 / 30", "Page 12 of 30", "p. 12", "xii"; not "0.25".
fn page_number(text: &str) -> bool {
    let decimal = text.chars().collect::<Vec<_>>().windows(3).any(|w| {
        w[0].is_ascii_digit() && matches!(w[1], '.' | ',') && w[2].is_ascii_digit()
    });
    if decimal {
        return false;
    }
    let lower: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && !"-‐–—−|/.,・".contains(*c))
        .collect::<String>()
        .to_lowercase();
    let rest = lower.strip_prefix("page").or_else(|| lower.strip_prefix('p')).unwrap_or(&lower);
    let rest = rest.replacen("of", "", 1);
    let digits = !rest.is_empty() && rest.chars().count() <= 8 && rest.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
    let roman = !rest.is_empty() && rest.len() <= 7 && rest.chars().all(|c| "ivxlcdm".contains(c));
    digits || roman
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_numbers() {
        for t in ["12", "- 12 -", "12 / 30", "Page 12 of 30", "p. 12", "xii", "３"] {
            assert!(page_number(t), "{t}");
        }
        for t in ["", "Fig. 1", "(1)", "2026年", "Vol. 3", "page", "12 Introduction", "0.025", "1,000"] {
            assert!(!page_number(t), "{t}");
        }
    }

    #[test]
    fn samples_cover_the_document_in_pairs() {
        assert_eq!(sample_pages(3), vec![0, 1, 2]);
        let s = sample_pages(200);
        assert_eq!(s.len(), 12);
        assert_eq!(s[..2], [1, 2]);
        assert_eq!(s[10..], [198, 199]);
    }
}
