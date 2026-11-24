//! Finding characters from positions: those under an annotation's quadrilaterals,
//! the one nearest to a point, and the insertion point of a caret.

use crate::TextModel;

/// Four corners in the order PDF writers use in /QuadPoints: top-left,
/// top-right, bottom-left, bottom-right; user space.
pub type QuadPoints = [(f32, f32); 4];

impl TextModel {
    /// Characters whose anchor lies inside any of the quadrilaterals, in reading order.
    pub fn chars_in_quads(&self, quads: &[QuadPoints]) -> Vec<usize> {
        self.chars
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                let (x, y) = c.anchor;
                quads.iter().any(|q| inside_quad(q, x, y))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The character whose box is closest to a user-space point.
    pub fn nearest_char(&self, x: f32, y: f32) -> Option<usize> {
        self.chars
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.rect.distance_to(x, y).total_cmp(&b.rect.distance_to(x, y)))
            .map(|(i, _)| i)
    }

    /// Reading position of an insertion mark at (x, y): the index of the
    /// character the mark stands before. Equals `chars.len()` after the last one.
    pub fn insertion_point(&self, x: f32, y: f32) -> Option<usize> {
        let (dx, dy) = self.frame.to_display(x, y);
        let line = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.display.distance_to(dx, dy).total_cmp(&b.display.distance_to(dx, dy)))
            .map(|(i, _)| i)?;
        let range = self.lines[line].chars.clone();
        let before = range
            .clone()
            .find(|&i| self.chars[i].display.center().0 > dx)
            .unwrap_or(range.end);
        Some(before)
    }
}

fn inside_quad(q: &QuadPoints, x: f32, y: f32) -> bool {
    // Walk the outline top-left, top-right, bottom-right, bottom-left.
    let outline = [q[0], q[1], q[3], q[2]];
    let mut sign = 0.0f32;
    for k in 0..4 {
        let (ax, ay) = outline[k];
        let (bx, by) = outline[(k + 1) % 4];
        let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_trapezoid() {
        let q = [(0.0, 10.0), (10.0, 12.0), (0.0, 0.0), (10.0, -2.0)];
        assert!(inside_quad(&q, 5.0, 5.0));
        assert!(inside_quad(&q, 9.0, 11.0));
        assert!(!inside_quad(&q, 1.0, 11.0));
        assert!(!inside_quad(&q, 11.0, 5.0));
    }
}
