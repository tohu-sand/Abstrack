//! Parts of a formula set one above the other: the numerator and the
//! denominator of a fraction. The part that does not share the line's band
//! comes out of line grouping as a short line of its own, which reading order
//! would put after the rest of the line (an equation number, the rest of a
//! sentence). It joins the line instead, right after the part it is stacked
//! on, so that a drag over the fraction takes in nothing else.

use crate::layout::LineGroup;
use crate::segment::Segment;

/// A short line counts as a stacked part when it sits centred on a short
/// segment of another line, close above or below it, and that line has text
/// beside the segment whose middle lies between the two in height (the main
/// text of the formula). Rows of a table and lines of a paragraph have
/// nothing in between.
pub(crate) fn attach(segments: &[Segment], lines: &mut Vec<LineGroup>, body: f32) {
    let bounds = |ids: &[usize]| {
        ids.iter()
            .map(|&i| segments[i].rect)
            .reduce(|a, b| a.union(&b))
    };
    let mut moves: Vec<(usize, usize, usize)> = Vec::new();
    let mut hosts = vec![false; lines.len()];
    let mut moved = vec![false; lines.len()];
    for i in 0..lines.len() {
        let Some(part) = bounds(&lines[i].segments) else { continue };
        let small = lines[i].segments.iter().all(|&s| segments[s].upright && segments[s].size <= 1.1 * body);
        if hosts[i] || part.width() > 6.0 * body || !small {
            continue;
        }
        let host = (0..lines.len()).filter(|&j| j != i && !moved[j]).find_map(|j| {
            lines[j].segments.iter().copied().find(|&m| {
                let on = &segments[m].rect;
                let centred = (on.center().0 - part.center().0).abs() <= 0.5 * on.width().max(part.width());
                let short = on.width() <= 3.0 * part.width() + 2.0 * body;
                let gap = (on.y0 - part.y1).max(part.y0 - on.y1);
                let close = gap >= -0.3 * body && gap <= 0.8 * body;
                if !(centred && short && close) {
                    return false;
                }
                let middle = (on.center().1 + part.center().1) / 2.0;
                let spread = (on.center().1 - part.center().1).abs();
                lines[j].segments.iter().any(|&a| {
                    let r = &segments[a].rect;
                    a != m
                        && (r.center().1 - middle).abs() <= 0.3 * spread
                        && r.horizontal_overlap(on) <= 0.0
                        && (r.x0 - on.x1).max(on.x0 - r.x1) <= 0.6 * body
                })
            })
            .map(|m| (j, m))
        });
        if let Some((j, m)) = host {
            moves.push((i, j, m));
            moved[i] = true;
            hosts[j] = true;
        }
    }
    for &(i, j, m) in &moves {
        let parts = std::mem::take(&mut lines[i].segments);
        lines[j].stacked.extend(parts.iter().map(|&s| (m, s)));
        lines[j].segments.extend(parts);
    }
    let mut k = 0;
    lines.retain(|_| {
        k += 1;
        !moved[k - 1]
    });
}
