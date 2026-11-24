//! Runs of characters that sit side by side on one baseline region, in content order.

use crate::geometry::{PageFrame, Rect};
use crate::InputChar;

/// A character prepared for layout analysis.
#[derive(Debug, Clone)]
pub(crate) struct Glyph {
    pub ch: char,
    pub user: Rect,
    pub display: Rect,
    pub anchor: (f32, f32),
    pub size: f32,
    pub source: usize,
    pub space_before: bool,
    pub upright: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Segment {
    pub glyphs: std::ops::Range<usize>,
    pub rect: Rect,
    /// Vertical extent of the full-size glyphs, so that sub- and superscripts do not stretch it.
    pub band: (f32, f32),
    pub size: f32,
    pub upright: bool,
}

const UPRIGHT_TOLERANCE_DEG: f32 = 10.0;

pub(crate) fn glyphs(input: &[InputChar], frame: &PageFrame) -> Vec<Glyph> {
    let mut out: Vec<Glyph> = Vec::with_capacity(input.len());
    for c in input {
        let user = Rect::from_array(c.bbox);
        if !(user.x0.is_finite() && user.y0.is_finite() && user.x1.is_finite() && user.y1.is_finite()) {
            continue;
        }
        // A visible glyph whose font maps it to a control code (a broken
        // ToUnicode table) is still on the page and can be selected.
        let ch = if c.ch.is_control() || c.ch == '\u{FFFE}' || c.ch == '\u{FFFF}' {
            if user.area() <= 0.0 {
                continue;
            }
            char::REPLACEMENT_CHARACTER
        } else {
            c.ch
        };
        let display = frame.rect_to_display(&user);
        let angle = frame.display_angle(c.angle);
        let upright = angle <= UPRIGHT_TOLERANCE_DEG || angle >= 360.0 - UPRIGHT_TOLERANCE_DEG;
        let size = if c.size > 0.0 { c.size } else { display.height().max(1.0) };
        // Text drawn twice at almost the same place (a common way to fake bold) counts once.
        let duplicate = out.iter().rev().take(4).any(|g| {
            g.ch == ch && g.display.horizontal_overlap(&display) * g.display.vertical_overlap(&display)
                >= 0.8 * display.area().max(f32::EPSILON)
        });
        if duplicate {
            continue;
        }
        out.push(Glyph {
            ch,
            user,
            display,
            anchor: anchor(&user, c.origin, c.angle, size),
            size,
            source: c.source,
            space_before: c.space_before,
            upright,
        });
    }
    out
}

/// The middle of the glyph's ink: a third of an em above the baseline for
/// upright text, otherwise the centre of the box.
fn anchor(user: &Rect, origin: Option<(f32, f32)>, angle: f32, size: f32) -> (f32, f32) {
    let (cx, cy) = user.center();
    let upright = angle.rem_euclid(360.0) <= UPRIGHT_TOLERANCE_DEG
        || angle.rem_euclid(360.0) >= 360.0 - UPRIGHT_TOLERANCE_DEG;
    match origin {
        Some((_, oy)) if upright && oy >= user.y0 - 0.5 * size && oy <= user.y1 => {
            (cx, (oy + 0.33 * size).clamp(user.y0, user.y1))
        }
        _ => (cx, cy),
    }
}

pub(crate) fn segments(glyphs: &[Glyph]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for (i, g) in glyphs.iter().enumerate() {
        let joins = out.last().is_some_and(|s| continues(s, &glyphs[i - 1], g));
        if joins {
            let s = out.last_mut().unwrap();
            s.glyphs.end = i + 1;
            s.rect = s.rect.union(&g.display);
            if g.size >= 0.85 * s.size {
                if g.size > s.size * 1.15 {
                    s.band = (g.display.y0, g.display.y1);
                } else {
                    s.band = (s.band.0.min(g.display.y0), s.band.1.max(g.display.y1));
                }
                s.size = s.size.max(g.size);
            }
        } else {
            out.push(Segment {
                glyphs: i..i + 1,
                rect: g.display,
                band: (g.display.y0, g.display.y1),
                size: g.size,
                upright: g.upright,
            });
        }
    }
    out
}

fn continues(s: &Segment, last: &Glyph, g: &Glyph) -> bool {
    if s.upright != g.upright {
        return false;
    }
    let size = s.size.max(g.size);
    if !g.upright {
        return last.display.distance_to(g.display.center().0, g.display.center().1) <= size;
    }
    let band = Rect::new(s.rect.x0, s.band.0, s.rect.x1, s.band.1);
    let overlap = band.vertical_overlap(&g.display);
    let shorter = g.display.height().min(band.height()).max(f32::EPSILON);
    if overlap < 0.4 * shorter {
        return false;
    }
    // Wholly to the left of the previous glyph: a new line, or another column.
    if g.display.x1 < last.display.x0 + 0.1 * size {
        return false;
    }
    g.display.x0 - s.rect.x1 <= 1.2 * size
}
