//! Reading order of segments: running headers first, then columns found from
//! the horizontal distribution of the text, bands cut by elements that cross a
//! column gap, lines top to bottom, and footers last.

use crate::Role;
use crate::geometry::Rect;
use crate::margins::Margins;
use crate::segment::Segment;
use crate::stacked;

/// Segments that form one line, left to right.
#[derive(Debug, Clone)]
pub(crate) struct LineGroup {
    pub segments: Vec<usize>,
    /// (host, part): segment `part` is set above or below segment `host`, as
    /// the two parts of a fraction, and is read right after it (see `stacked`).
    pub stacked: Vec<(usize, usize)>,
}

/// Lines of one region without columns, top to bottom.
#[derive(Debug, Clone)]
pub(crate) struct BlockGroup {
    pub lines: Vec<LineGroup>,
    pub role: Role,
}

/// The type size of the running text: the size of the median upright
/// character, so that the many short labels of a figure do not pull it down.
pub(crate) fn body_size(segments: &[Segment]) -> f32 {
    let mut sized: Vec<(f32, usize)> = segments
        .iter()
        .filter(|s| s.upright && s.size.is_finite())
        .map(|s| (s.size, s.glyphs.len()))
        .collect();
    sized.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = sized.iter().map(|s| s.1).sum::<usize>() / 2;
    let mut seen = 0;
    for (size, count) in sized {
        seen += count;
        if seen > half {
            return size;
        }
    }
    10.0
}

pub(crate) fn order(segments: &[Segment], body: f32, margins: Margins) -> Vec<BlockGroup> {
    let mut marginal = vec![false; segments.len()];
    for &i in margins.header.iter().chain(&margins.footer) {
        marginal[i] = true;
    }
    let rest: Vec<usize> = (0..segments.len()).filter(|&i| !marginal[i]).collect();
    let mut out = Vec::new();
    if !margins.header.is_empty() {
        out.push(BlockGroup {
            lines: lines(segments, margins.header, body),
            role: Role::Header,
        });
    }
    split(segments, rest, body, None, 0, &mut out);
    if !margins.footer.is_empty() {
        out.push(BlockGroup {
            lines: lines(segments, margins.footer, body),
            role: Role::Footer,
        });
    }
    out
}

/// `inherited` is the gutter found for the enclosing region. A band cut out
/// of it may hold too few lines to show the gutter by itself, but still has
/// the same columns.
fn split(
    segments: &[Segment],
    ids: Vec<usize>,
    body: f32,
    inherited: Option<(f32, f32)>,
    depth: usize,
    out: &mut Vec<BlockGroup>,
) {
    if ids.is_empty() {
        return;
    }
    let gutter = if ids.len() > 1 && depth < 12 {
        find_gutter(segments, &ids, body).or(inherited)
    } else {
        None
    };
    if let Some(gutter) = gutter {
        let (mut left, mut right, mut spanning) = (Vec::new(), Vec::new(), Vec::new());
        for &i in &ids {
            let r = &segments[i].rect;
            // What lies well across the middle of the gap spans the columns
            // (a full-width line, a cell of a full-width table); a line or a
            // label that only strays into the gap belongs to its column.
            let centre = (gutter.0 + gutter.1) / 2.0;
            let into_left = centre - r.x0;
            let into_right = r.x1 - centre;
            let across = into_left.min(into_right) >= (0.25 * r.width()).max(0.5 * body);
            if segments[i].upright && across {
                spanning.push(i);
            } else if into_left >= into_right {
                left.push(i);
            } else {
                right.push(i);
            }
        }
        if spanning.is_empty() && !left.is_empty() && !right.is_empty() {
            split(segments, left, body, None, depth + 1, out);
            split(segments, right, body, None, depth + 1, out);
            return;
        }
        if !spanning.is_empty() {
            let bands = bands(segments, &ids, &spanning);
            if bands.len() > 1 {
                for band in bands {
                    split(segments, band, body, Some(gutter), depth + 1, out);
                }
                return;
            }
        }
    }
    out.push(BlockGroup {
        lines: lines(segments, ids, body),
        role: Role::Body,
    });
}

/// A vertical strip free of text that separates two columns, as (left edge, right edge).
/// Only lines of some length define it; a page number or a label that sits in
/// the strip is then read as something that crosses it.
fn find_gutter(segments: &[Segment], ids: &[usize], body: f32) -> Option<(f32, f32)> {
    let mut long: Vec<&Segment> = ids
        .iter()
        .map(|&i| &segments[i])
        .filter(|s| s.upright && s.rect.width() >= 3.0 * body)
        .collect();
    if long.len() < 6 {
        return None;
    }
    let mut edges: Vec<f32> = long.iter().flat_map(|s| [s.rect.x0, s.rect.x1]).collect();
    edges.sort_by(f32::total_cmp);
    edges.dedup_by(|a, b| (*a - *b).abs() < 0.25);

    // Filtering in height order leaves both sides ready for their quantiles.
    long.sort_unstable_by(|a, b| a.rect.center().1.total_cmp(&b.rect.center().1));
    let (mut left, mut right, mut crossing) = (Vec::new(), Vec::new(), Vec::new());
    let mut best: Option<(f32, (f32, f32))> = None;
    for pair in edges.windows(2) {
        let x = (pair[0] + pair[1]) / 2.0;
        left.clear();
        right.clear();
        crossing.clear();
        for &s in &long {
            if s.rect.x1 <= x {
                left.push(s);
            } else if s.rect.x0 >= x {
                right.push(s);
            } else {
                crossing.push(s);
            }
        }
        if left.len() < 3 || right.len() < 3 {
            continue;
        }
        let g0 = left.iter().map(|s| s.rect.x1).fold(f32::NEG_INFINITY, f32::max);
        let g1 = right.iter().map(|s| s.rect.x0).fold(f32::INFINITY, f32::min);
        if g1 - g0 < 0.8 * body {
            continue;
        }
        // The two sides must stand side by side. Only lines that cut through
        // them count against them, not a title and an abstract set full width
        // above: those within the heights of either side, leaving out its top
        // and bottom tenth.
        let (left_lo, left_hi) = (quantile(&left, 0.1), quantile(&left, 0.9));
        let (right_lo, right_hi) = (quantile(&right, 0.1), quantile(&right, 0.9));
        if left_lo.max(right_lo) >= left_hi.min(right_hi) {
            continue;
        }
        let cross = crossing
            .iter()
            .filter(|s| {
                let y = s.rect.center().1;
                (left_lo..=left_hi).contains(&y) || (right_lo..=right_hi).contains(&y)
            })
            .count();
        if cross * 3 > left.len() + right.len() {
            continue;
        }
        // The lines of a column start at its left edge. The rest of lines
        // broken by formulas at about the same place start anywhere. Labels
        // of a figure in the column, set smaller, do not count.
        let text = right.iter().filter(|s| (s.size - body).abs() <= 0.1 * body);
        let starting = text.clone().filter(|s| s.rect.x0 - g1 <= 0.5 * body).count();
        if starting * 2 < text.count() {
            continue;
        }
        // Columns of running text: their lines are long compared to the type size.
        // This keeps the cells of a table from being read as columns.
        let wide = |side: &[&Segment]| median(side.iter().map(|s| s.rect.width())).unwrap_or(0.0) >= 8.0 * body;
        if !wide(&left) || !wide(&right) {
            continue;
        }
        let score = left.len().min(right.len()) as f32 - 3.0 * cross as f32 + (g1 - g0) / body * 0.01;
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, (g0, g1)));
        }
    }
    best.map(|(_, g)| g)
}

/// Cuts a region into horizontal bands at the elements that cross the gutter:
/// the columns above, the crossing elements, the columns below, and so on.
fn bands(segments: &[Segment], ids: &[usize], spanning: &[usize]) -> Vec<Vec<usize>> {
    let mut spans: Vec<(f32, f32)> = spanning
        .iter()
        .map(|&i| (segments[i].rect.y0, segments[i].rect.y1))
        .collect();
    spans.sort_by(|a, b| b.1.total_cmp(&a.1));
    let columnar: Vec<usize> = ids.iter().copied().filter(|i| !spanning.contains(i)).collect();
    // Crossing elements one after another, with nothing of the columns
    // between them (the lines of an abstract or a caption), make one band.
    let columns_between = |bottom: f32, top: f32| {
        columnar.iter().any(|&i| {
            let c = segments[i].rect.center().1;
            c < bottom && c > top
        })
    };
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (y0, y1) in spans {
        match merged.last_mut() {
            Some(last) if y1 >= last.0 || !columns_between(last.0, y1) => last.0 = last.0.min(y0),
            _ => merged.push((y0, y1)),
        }
    }
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut remaining = columnar;
    for &(y0, y1) in &merged {
        let (above, rest): (Vec<usize>, Vec<usize>) =
            remaining.into_iter().partition(|&i| segments[i].rect.center().1 > y1);
        remaining = rest;
        if !above.is_empty() {
            out.push(above);
        }
        let mut band: Vec<usize> = spanning
            .iter()
            .copied()
            .filter(|&i| {
                let c = segments[i].rect.center().1;
                c >= y0 && c <= y1
            })
            .collect();
        let (inside, rest): (Vec<usize>, Vec<usize>) =
            remaining.into_iter().partition(|&i| segments[i].rect.center().1 >= y0);
        remaining = rest;
        band.extend(inside);
        out.push(band);
    }
    if !remaining.is_empty() {
        out.push(remaining);
    }
    out
}

/// Groups the segments of a column into lines. Sub- and superscripts overlap
/// the line they belong to and join it; text far to the side does not.
fn lines(segments: &[Segment], mut ids: Vec<usize>, body: f32) -> Vec<LineGroup> {
    ids.sort_by(|&a, &b| {
        let (ra, rb) = (&segments[a].rect, &segments[b].rect);
        band_top(&segments[b]).total_cmp(&band_top(&segments[a])).then(ra.x0.total_cmp(&rb.x0))
    });
    struct Open {
        band: (f32, f32),
        size: f32,
        members: Vec<usize>,
    }
    let mut open: Vec<Open> = Vec::new();
    for i in ids {
        let s = &segments[i];
        let seg_band = Rect::new(s.rect.x0, s.band.0, s.rect.x1, s.band.1);
        let target = open.iter().position(|l| {
            let line_band = Rect::new(0.0, l.band.0, 1.0, l.band.1);
            let shorter = seg_band.height().min(line_band.height()).max(f32::EPSILON);
            let overlaps = line_band.vertical_overlap(&s.rect) >= 0.4 * s.rect.height().min(line_band.height())
                || line_band.vertical_overlap(&seg_band) >= 0.5 * shorter;
            if !overlaps {
                return false;
            }
            let stacked = l
                .members
                .iter()
                .any(|&m| segments[m].rect.horizontal_overlap(&s.rect) > 0.3 * segments[m].rect.width().min(s.rect.width()));
            let near = l.members.iter().any(|&m| {
                let r = &segments[m].rect;
                (s.rect.x0 - r.x1).max(r.x0 - s.rect.x1) <= 4.0 * body
            });
            !stacked && near
        });
        match target {
            Some(t) => {
                let l = &mut open[t];
                if s.size > l.size * 1.15 {
                    l.band = s.band;
                    l.size = s.size;
                } else if s.size >= 0.85 * l.size {
                    l.band = (l.band.0.min(s.band.0), l.band.1.max(s.band.1));
                }
                l.members.push(i);
            }
            None => open.push(Open {
                band: s.band,
                size: s.size,
                members: vec![i],
            }),
        }
    }
    open.sort_by(|a, b| b.band.1.total_cmp(&a.band.1));
    let mut out: Vec<LineGroup> = open
        .into_iter()
        .map(|mut l| {
            l.members
                .sort_by(|&a, &b| segments[a].rect.x0.total_cmp(&segments[b].rect.x0));
            LineGroup {
                segments: l.members,
                stacked: Vec::new(),
            }
        })
        .collect();
    stacked::attach(segments, &mut out, body);
    out
}

/// The height below which a fraction `q` of the segments' centres lie.
fn quantile(segments: &[&Segment], q: f32) -> f32 {
    segments[((segments.len() - 1) as f32 * q).round() as usize].rect.center().1
}

fn band_top(s: &Segment) -> f32 {
    s.band.1
}

fn median(values: impl Iterator<Item = f32>) -> Option<f32> {
    let mut v: Vec<f32> = values.filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}
