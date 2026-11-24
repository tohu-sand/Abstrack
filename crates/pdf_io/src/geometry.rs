//! Page boxes and rotation, with inheritance through the page tree
//! (ISO 32000-1, 7.7.3.4), the way viewers use them for display.

use crate::{PdfFile, resolve};
use lopdf::{Dictionary, Object, ObjectId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    pub media: [f64; 4],
    /// Visible area: the crop box clipped to the media box, in user space.
    pub visible: [f64; 4],
    /// Clockwise display rotation: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Size of one user-space unit in points (/UserUnit, PDF 1.6).
    pub user_unit: f64,
}

impl PageGeometry {
    /// Displayed size in points, rotation and /UserUnit applied.
    pub fn display_size(&self) -> (f64, f64) {
        let w = (self.visible[2] - self.visible[0]) * self.user_unit;
        let h = (self.visible[3] - self.visible[1]) * self.user_unit;
        if self.rotation.is_multiple_of(180) { (w, h) } else { (h, w) }
    }
}

const LETTER: [f64; 4] = [0.0, 0.0, 612.0, 792.0];

impl PdfFile {
    /// A missing or malformed media box gives US Letter; a missing or
    /// malformed crop box, or one outside the media box, gives the media box.
    /// /Rotate is taken modulo 360 and rounded down to a multiple of 90.
    /// /UserUnit is read from the page only, as it is not inheritable
    /// (ISO 32000-1, table 30).
    pub fn page_geometry(&self, page_id: ObjectId) -> PageGeometry {
        let media = self
            .inherited(page_id, b"MediaBox")
            .and_then(|o| rect(self, o))
            .unwrap_or(LETTER);
        let crop = self
            .inherited(page_id, b"CropBox")
            .and_then(|o| rect(self, o))
            .unwrap_or(media);
        let mut visible = [
            crop[0].max(media[0]),
            crop[1].max(media[1]),
            crop[2].min(media[2]),
            crop[3].min(media[3]),
        ];
        if visible[2] <= visible[0] || visible[3] <= visible[1] {
            visible = media;
        }
        let rotation = self
            .inherited(page_id, b"Rotate")
            .and_then(|o| resolve(&self.doc, o).as_i64().ok())
            .map(|r| (r.rem_euclid(360) / 90 * 90) as u16)
            .unwrap_or(0);
        let user_unit = self
            .doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"UserUnit").ok())
            .and_then(|o| number(resolve(&self.doc, o)))
            .filter(|u| *u > 0.0)
            .unwrap_or(1.0);
        PageGeometry {
            media,
            visible,
            rotation,
            user_unit,
        }
    }

    /// A page attribute from the page or its nearest ancestor that has it.
    fn inherited(&self, page_id: ObjectId, key: &[u8]) -> Option<&Object> {
        let mut dict: &Dictionary = self.doc.get_dictionary(page_id).ok()?;
        for _ in 0..64 {
            if let Ok(value) = dict.get(key) {
                return Some(value);
            }
            let parent = dict.get(b"Parent").ok()?;
            dict = resolve(&self.doc, parent).as_dict().ok()?;
        }
        None
    }
}

fn number(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn rect(file: &PdfFile, o: &Object) -> Option<[f64; 4]> {
    let items = resolve(&file.doc, o).as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let v: Vec<f64> = items
        .iter()
        .filter_map(|i| number(resolve(&file.doc, i)))
        .collect();
    if v.len() != 4 {
        return None;
    }
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}
