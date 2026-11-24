//! Changing what an existing annotation marks. A text markup takes new
//! QuadPoints, and the insertion mark of a replacement moves to the new end of
//! the text it replaces. The /Rect and the appearance follow, /M records the
//! change, and every other key is kept as it was.

use crate::appearance::{Corners, numbers};
use crate::date::PdfDate;
use crate::fields::date;
use crate::write::{
    CaretMark, DEFAULT_COLORS, HIGHLIGHT_OPACITY, Markup, caret_look, markup_look, quad_points, set_appearance,
};
use pdf_io::{Dictionary, Object, ObjectId};

/// The objects to write so that the text markup `annotation` (object `id`)
/// marks `quads`: its new appearance, then the annotation itself.
pub fn markup(
    id: ObjectId,
    annotation: &Dictionary,
    quads: &[Corners],
    now: PdfDate,
    next_id: &mut dyn FnMut() -> ObjectId,
) -> Result<Vec<(ObjectId, Object)>, String> {
    let subtype = annotation.get(b"Subtype").and_then(Object::as_name).map_err(|_| "コメントの種類が分かりません".to_string())?;
    let (kind, default) = match subtype {
        b"Highlight" => (Markup::Highlight, DEFAULT_COLORS.highlight),
        b"StrikeOut" => (Markup::StrikeOut, DEFAULT_COLORS.strike_out),
        b"Underline" => (Markup::Underline, DEFAULT_COLORS.underline),
        _ => return Err("この種類のコメントは範囲を変えられません".into()),
    };
    if quads.is_empty() {
        return Err("範囲に文字がありません".into());
    }
    let mut dict = annotation.clone();
    let color = colour(&dict).unwrap_or(default);
    let opacity = dict.get(b"CA").ok().and_then(number).unwrap_or(HIGHLIGHT_OPACITY);
    let (rect, mut objects) = markup_look(kind, quads, color, opacity, next_id);
    dict.set("QuadPoints", quad_points(quads));
    dict.set("Rect", numbers(&rect));
    dict.set("M", date(now));
    set_appearance(&mut dict, objects[0].0);
    objects.push((id, Object::Dictionary(dict)));
    Ok(objects)
}

/// The objects to write so that the insertion mark `annotation` (object
/// `id`) stands at `caret`: its new appearance, then the mark itself.
pub fn caret(
    id: ObjectId,
    annotation: &Dictionary,
    caret: CaretMark,
    now: PdfDate,
    next_id: &mut dyn FnMut() -> ObjectId,
) -> Vec<(ObjectId, Object)> {
    let mut dict = annotation.clone();
    let color = colour(&dict).unwrap_or(DEFAULT_COLORS.replace);
    let (rect, rd, look) = caret_look(caret, color, next_id);
    dict.set("Rect", numbers(&rect));
    dict.set("RD", numbers(&[rd, rd, rd, rd]));
    dict.set("M", date(now));
    set_appearance(&mut dict, look.0);
    vec![look, (id, Object::Dictionary(dict))]
}

/// /C as RGB: grey and CMYK are converted, so that the new appearance keeps
/// the colour the annotation had.
fn colour(dict: &Dictionary) -> Option<[f64; 3]> {
    let c: Vec<f64> = dict.get(b"C").ok()?.as_array().ok()?.iter().map(number).collect::<Option<_>>()?;
    crate::model::color_rgb(&c)
}

fn number(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}
