//! Rectangles in points, y up, and the mapping from a page's user space to its display orientation.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    pub fn from_array(r: [f32; 4]) -> Rect {
        Rect::new(r[0], r[1], r[2], r[3])
    }

    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    pub fn center(&self) -> (f32, f32) {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }

    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    pub fn vertical_overlap(&self, other: &Rect) -> f32 {
        (self.y1.min(other.y1) - self.y0.max(other.y0)).max(0.0)
    }

    pub fn horizontal_overlap(&self, other: &Rect) -> f32 {
        (self.x1.min(other.x1) - self.x0.max(other.x0)).max(0.0)
    }

    pub fn area(&self) -> f32 {
        self.width().max(0.0) * self.height().max(0.0)
    }

    /// Distance from a point to the rectangle (0 inside).
    pub fn distance_to(&self, x: f32, y: f32) -> f32 {
        let dx = (self.x0 - x).max(0.0).max(x - self.x1);
        let dy = (self.y0 - y).max(0.0).max(y - self.y1);
        (dx * dx + dy * dy).sqrt()
    }
}

/// The visible area of a page and its clockwise display rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFrame {
    pub bbox: Rect,
    /// 0, 90, 180 or 270.
    pub rotation: u16,
}

impl PageFrame {
    pub fn upright(bbox: Rect) -> PageFrame {
        PageFrame { bbox, rotation: 0 }
    }

    /// Maps a user-space point into display space: the page as shown, with its
    /// visible area starting at (0, 0), y still pointing up.
    pub fn to_display(&self, x: f32, y: f32) -> (f32, f32) {
        let b = &self.bbox;
        match self.rotation {
            90 => (y - b.y0, b.x1 - x),
            180 => (b.x1 - x, b.y1 - y),
            270 => (b.y1 - y, x - b.x0),
            _ => (x - b.x0, y - b.y0),
        }
    }

    /// The inverse of [`PageFrame::to_display`].
    pub fn from_display(&self, x: f32, y: f32) -> (f32, f32) {
        let b = &self.bbox;
        match self.rotation {
            90 => (b.x1 - y, x + b.y0),
            180 => (b.x1 - x, b.y1 - y),
            270 => (y + b.x0, b.y1 - x),
            _ => (x + b.x0, y + b.y0),
        }
    }

    pub fn rect_to_display(&self, r: &Rect) -> Rect {
        let (ax, ay) = self.to_display(r.x0, r.y0);
        let (bx, by) = self.to_display(r.x1, r.y1);
        Rect::new(ax, ay, bx, by)
    }

    /// Glyph angle (counter-clockwise, degrees) as seen on the display.
    pub fn display_angle(&self, angle: f32) -> f32 {
        (angle - self.rotation as f32).rem_euclid(360.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_maps_corners() {
        let frame = PageFrame {
            bbox: Rect::new(0.0, 0.0, 100.0, 200.0),
            rotation: 90,
        };
        // The top-left corner of the page ends up at the top-right when turned clockwise.
        assert_eq!(frame.to_display(0.0, 200.0), (200.0, 100.0));
        assert_eq!(frame.to_display(100.0, 200.0), (200.0, 0.0));
        let frame = PageFrame { rotation: 270, ..frame };
        assert_eq!(frame.to_display(0.0, 200.0), (0.0, 0.0));
        assert_eq!(frame.display_angle(90.0), 180.0);
    }

    #[test]
    fn from_display_undoes_to_display() {
        for rotation in [0, 90, 180, 270] {
            let frame = PageFrame {
                bbox: Rect::new(10.0, 20.0, 110.0, 220.0),
                rotation,
            };
            let (dx, dy) = frame.to_display(30.0, 50.0);
            assert_eq!(frame.from_display(dx, dy), (30.0, 50.0), "rotation {rotation}");
        }
    }
}
