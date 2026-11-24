//! New annotations, and changes to existing ones, in the form Acrobat writes
//! them: the annotation dictionary with its /Popup and its appearance stream.

use crate::appearance::{self, Corners, numbers};
use crate::date::PdfDate;
use crate::fields::{date, set_contents, uuid};
use pdf_io::{Dictionary, Object, ObjectId, text_string};

/// What a new comment marks.
#[derive(Debug, Clone)]
pub enum NewComment {
    Highlight { quads: Vec<Corners> },
    /// Text to delete.
    StrikeOut { quads: Vec<Corners> },
    Underline { quads: Vec<Corners> },
    /// Text to replace: struck out, with an insertion mark after it that
    /// carries the replacement.
    Replace { quads: Vec<Corners>, caret: CaretMark },
    Insert { caret: CaretMark },
    /// A note at a point (its icon's top-left corner).
    Note { at: (f64, f64) },
    /// A comment on an area of the page.
    Area { rect: [f64; 4] },
    /// A reply to the annotation `to`, whose /Rect is `target`.
    Reply { to: ObjectId, target: [f64; 4] },
}

/// Where an insertion mark goes: the insertion point, the bottom of the line
/// and the height of the line's quadrilateral, user space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretMark {
    pub x: f64,
    pub bottom: f64,
    pub height: f64,
}

/// Who writes, when, on which page, and in which colors.
pub struct Context<'a> {
    pub page: ObjectId,
    /// The right edge of the page's media box: Acrobat puts pop-up windows
    /// just outside it.
    pub page_right: f64,
    pub author: &'a str,
    pub now: PdfDate,
    pub colors: Colors,
}

/// The color (/C, RGB) of each kind of new comment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub highlight: [f64; 3],
    /// The strike-out and the insertion mark of a replacement.
    pub replace: [f64; 3],
    /// A strike-out alone: text to delete.
    pub strike_out: [f64; 3],
    pub underline: [f64; 3],
    pub insert: [f64; 3],
    /// Notes, and replies.
    pub note: [f64; 3],
    pub area: [f64; 3],
}

/// The colors of the reviews in ref/, so that new comments look like the
/// earlier ones; the kinds those never use take the colors Acrobat gives
/// them on the machine the samples came from.
pub const DEFAULT_COLORS: Colors = Colors {
    highlight: [1.0, 0.819611, 0.0],
    replace: [0.972549, 0.392151, 0.392151],
    strike_out: [0.972549, 0.392151, 0.392151],
    underline: [0.0745087, 0.450974, 0.909805],
    insert: [0.752945, 0.215683, 0.768631],
    note: [1.0, 0.819611, 0.0],
    area: [0.858826, 0.203918, 0.145096],
};

impl Default for Colors {
    fn default() -> Colors {
        DEFAULT_COLORS
    }
}

/// The new objects of one comment.
#[derive(Debug, Clone)]
pub struct Built {
    pub objects: Vec<(ObjectId, Object)>,
    /// To append to the page's /Annots, in this order.
    pub annots: Vec<ObjectId>,
    /// The annotation that carries the comment's text (the insertion mark of
    /// a replacement).
    pub main: ObjectId,
}

/// /CA of a highlight: Acrobat's default, as it writes it.
pub const HIGHLIGHT_OPACITY: f64 = 0.399994;

/// Pop-up windows as Acrobat 26 sizes them.
const POPUP_SIZE: (f64, f64) = (184.0, 92.0);
const ICON: f64 = 24.0;

pub fn build(comment: &NewComment, contents: &str, ctx: &Context<'_>, next_id: &mut dyn FnMut() -> ObjectId) -> Built {
    let mut out = Built { objects: Vec::new(), annots: Vec::new(), main: (0, 0) };
    let colors = ctx.colors;
    match comment {
        NewComment::Highlight { quads } => {
            out.main = markup(&mut out, ctx, next_id, Markup::Highlight, colors.highlight, quads, contents);
        }
        NewComment::StrikeOut { quads } => {
            out.main = markup(&mut out, ctx, next_id, Markup::StrikeOut, colors.strike_out, quads, contents);
        }
        NewComment::Underline { quads } => {
            out.main = markup(&mut out, ctx, next_id, Markup::Underline, colors.underline, quads, contents);
        }
        NewComment::Replace { quads, caret } => {
            // The strike-out goes first in /Annots and points to the mark.
            let strike = next_id();
            let mark = next_id();
            let kind = Markup::StrikeOut;
            markup_with_id(&mut out, ctx, next_id, strike, kind, colors.replace, quads, "", Some(mark));
            insertion(&mut out, ctx, next_id, mark, *caret, contents, true);
            out.main = mark;
        }
        NewComment::Insert { caret } => {
            let mark = next_id();
            insertion(&mut out, ctx, next_id, mark, *caret, contents, false);
            out.main = mark;
        }
        NewComment::Note { at } => {
            let rect = [at.0, at.1 - ICON, at.0 + ICON, at.1];
            out.main = note(&mut out, ctx, next_id, rect, contents, None);
        }
        NewComment::Area { rect } => {
            out.main = area(&mut out, ctx, next_id, *rect, contents);
        }
        NewComment::Reply { to, target } => {
            let rect = [target[0], target[3] - ICON, target[0] + ICON, target[3]];
            out.main = note(&mut out, ctx, next_id, rect, contents, Some(*to));
        }
    }
    out
}

/// The object to write so that the page's /Annots holds `ids` at the end
/// (`add`) or no longer holds them: the page itself, or the array when the
/// page refers to one.
pub fn change_annots(
    doc: &pdf_io::lopdf::Document,
    page: ObjectId,
    ids: &[ObjectId],
    add: bool,
) -> Result<(ObjectId, Object), String> {
    let page_dict = doc.get_dictionary(page).map_err(|e| format!("ページ {} {} を読めません: {e}", page.0, page.1))?;
    let (target, mut list) = match page_dict.get(b"Annots") {
        Ok(Object::Reference(array_id)) => {
            let array = doc
                .get_object(*array_id)
                .and_then(|o| o.as_array())
                .map_err(|e| format!("注釈の一覧 {} {} を読めません: {e}", array_id.0, array_id.1))?;
            (Some(*array_id), array.clone())
        }
        Ok(Object::Array(array)) => (None, array.clone()),
        _ => (None, Vec::new()),
    };
    if add {
        list.extend(ids.iter().map(|&id| Object::Reference(id)));
    } else {
        list.retain(|entry| !matches!(entry, Object::Reference(id) if ids.contains(id)));
    }
    Ok(match target {
        Some(array_id) => (array_id, Object::Array(list)),
        None => {
            let mut page_dict = page_dict.clone();
            page_dict.set("Annots", Object::Array(list));
            (page, Object::Dictionary(page_dict))
        }
    })
}

/// /Contents and /RC for `text` (both removed for an empty text), and /M.
/// Every other key of the annotation is kept as it was.
pub fn with_contents(annotation: &Dictionary, text: &str, now: PdfDate) -> Dictionary {
    let mut dict = annotation.clone();
    set_contents(&mut dict, text);
    dict.set("M", date(now));
    dict
}

/// The kinds of text markup this program writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Markup {
    Highlight,
    StrikeOut,
    Underline,
}

/// The /Rect and the appearance objects of a text markup over `quads`, the
/// normal appearance first.
pub(crate) fn markup_look(
    kind: Markup,
    quads: &[Corners],
    color: [f64; 3],
    opacity: f64,
    next_id: &mut dyn FnMut() -> ObjectId,
) -> ([f64; 4], Vec<(ObjectId, Object)>) {
    let rect = match kind {
        Markup::Highlight => around(quads, |h| (h / 32.0, h / 32.0)),
        Markup::StrikeOut | Markup::Underline => around(quads, |h| (0.3 * h, h / 16.0)),
    };
    let ap = next_id();
    let objects = match kind {
        Markup::Highlight => {
            let size = (rect[2] - rect[0], rect[3] - rect[1]);
            let (group, shapes) = (next_id(), next_id());
            vec![
                (ap, Object::Stream(appearance::highlight(size, opacity, group))),
                (group, Object::Stream(appearance::highlight_group(size, shapes))),
                (shapes, Object::Stream(appearance::highlight_quads(rect, quads, color))),
            ]
        }
        Markup::StrikeOut => vec![(ap, Object::Stream(appearance::line_through(rect, quads, color, 3.0 / 7.0)))],
        Markup::Underline => vec![(ap, Object::Stream(appearance::line_through(rect, quads, color, 1.0 / 7.0)))],
    };
    (rect, objects)
}

/// /QuadPoints for `quads`, the corners in Acrobat's order.
pub(crate) fn quad_points(quads: &[Corners]) -> Object {
    numbers(&quads.iter().flat_map(|q| q.iter().flat_map(|&(x, y)| [x, y])).collect::<Vec<_>>())
}

/// The /Rect, the margin (/RD on each side) and the appearance of an
/// insertion mark: 0.7 h wide, 0.55 h high, centred on the insertion point
/// and standing on the bottom of the line, with a margin of 0.055 h.
pub(crate) fn caret_look(
    caret: CaretMark,
    color: [f64; 3],
    next_id: &mut dyn FnMut() -> ObjectId,
) -> ([f64; 4], f64, (ObjectId, Object)) {
    let (w, h, rd) = (0.7 * caret.height, 0.55 * caret.height, 0.055 * caret.height);
    let rect = [caret.x - w / 2.0 - rd, caret.bottom - rd, caret.x + w / 2.0 + rd, caret.bottom + h + rd];
    (rect, rd, (next_id(), Object::Stream(appearance::caret(w, h, rd, color))))
}

#[allow(clippy::too_many_arguments)]
fn markup(
    out: &mut Built,
    ctx: &Context<'_>,
    next_id: &mut dyn FnMut() -> ObjectId,
    kind: Markup,
    color: [f64; 3],
    quads: &[Corners],
    contents: &str,
) -> ObjectId {
    let id = next_id();
    markup_with_id(out, ctx, next_id, id, kind, color, quads, contents, None);
    id
}

/// A text markup annotation. With `target`, the strike-out of a replacement:
/// grouped under the insertion mark.
#[allow(clippy::too_many_arguments)]
fn markup_with_id(
    out: &mut Built,
    ctx: &Context<'_>,
    next_id: &mut dyn FnMut() -> ObjectId,
    id: ObjectId,
    kind: Markup,
    color: [f64; 3],
    quads: &[Corners],
    contents: &str,
    target: Option<ObjectId>,
) {
    let (subtype, subject) = match kind {
        Markup::Highlight => ("Highlight", "ハイライト表示"),
        Markup::StrikeOut => ("StrikeOut", "取り消し線"),
        Markup::Underline => ("Underline", "下線"),
    };
    let (rect, look) = markup_look(kind, quads, color, HIGHLIGHT_OPACITY, next_id);
    let mut dict = common(ctx, subtype, subject, rect, color, contents);
    dict.set("QuadPoints", quad_points(quads));
    if kind == Markup::Highlight {
        dict.set("CA", Object::Real(HIGHLIGHT_OPACITY as f32));
    }
    set_appearance(&mut dict, look[0].0);
    out.objects.extend(look);
    if let Some(target) = target {
        dict.set("IRT", Object::Reference(target));
        dict.set("RT", Object::Name(b"Group".to_vec()));
        dict.set("IT", Object::Name(b"StrikeOutTextEdit".to_vec()));
    }
    let top = quads.first().map_or(rect[3], |q| q[0].1.max(q[1].1));
    finish(out, ctx, next_id, id, dict, top);
}

/// An insertion mark, of a replacement when `replace`.
fn insertion(
    out: &mut Built,
    ctx: &Context<'_>,
    next_id: &mut dyn FnMut() -> ObjectId,
    id: ObjectId,
    caret: CaretMark,
    contents: &str,
    replace: bool,
) {
    let color = if replace { ctx.colors.replace } else { ctx.colors.insert };
    let (rect, rd, look) = caret_look(caret, color, next_id);
    let mut dict = common(ctx, "Caret", "挿入テキスト", rect, color, contents);
    dict.set("RD", numbers(&[rd, rd, rd, rd]));
    if replace {
        dict.set("IT", Object::Name(b"Replace".to_vec()));
    }
    set_appearance(&mut dict, look.0);
    out.objects.push(look);
    finish(out, ctx, next_id, id, dict, rect[3] - rd);
}

/// A note, or with `reply_to` a reply: a Text annotation with a balloon icon.
fn note(
    out: &mut Built,
    ctx: &Context<'_>,
    next_id: &mut dyn FnMut() -> ObjectId,
    rect: [f64; 4],
    contents: &str,
    reply_to: Option<ObjectId>,
) -> ObjectId {
    let id = next_id();
    let mut dict = common(ctx, "Text", "ノート注釈", rect, ctx.colors.note, contents);
    dict.set("F", Object::Integer(28));
    dict.set("Name", Object::Name(b"Comment".to_vec()));
    if let Some(target) = reply_to {
        dict.set("IRT", Object::Reference(target));
    }
    let ap = next_id();
    out.objects.push((ap, Object::Stream(appearance::note_icon(ctx.colors.note))));
    set_appearance(&mut dict, ap);
    finish(out, ctx, next_id, id, dict, rect[3]);
    id
}

/// A rectangle drawn on the edge of `area`, with a margin of 0.5.
fn area(out: &mut Built, ctx: &Context<'_>, next_id: &mut dyn FnMut() -> ObjectId, area: [f64; 4], contents: &str) -> ObjectId {
    let id = next_id();
    let rd = 0.5;
    let pad = rd + 0.5;
    let rect = [area[0] - pad, area[1] - pad, area[2] + pad, area[3] + pad];
    let mut dict = common(ctx, "Square", "長方形", rect, ctx.colors.area, contents);
    dict.set("RD", numbers(&[rd, rd, rd, rd]));
    let ap = next_id();
    out.objects.push((ap, Object::Stream(appearance::square(rect, rd, ctx.colors.area))));
    set_appearance(&mut dict, ap);
    finish(out, ctx, next_id, id, dict, rect[3]);
    id
}

/// The keys every comment has.
pub(crate) fn common(ctx: &Context<'_>, subtype: &str, subject: &str, rect: [f64; 4], color: [f64; 3], contents: &str) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Annot".to_vec()));
    dict.set("Subtype", Object::Name(subtype.as_bytes().to_vec()));
    dict.set("Rect", numbers(&rect));
    dict.set("C", numbers(&color));
    dict.set("F", Object::Integer(4));
    dict.set("P", Object::Reference(ctx.page));
    dict.set("T", text_string(ctx.author));
    dict.set("Subj", Object::String(pdf_io::encode_text(subject), pdf_io::StringFormat::Hexadecimal));
    dict.set("NM", Object::String(uuid().into_bytes(), pdf_io::StringFormat::Literal));
    dict.set("CreationDate", date(ctx.now));
    dict.set("M", date(ctx.now));
    set_contents(&mut dict, contents);
    dict
}

/// Stores the annotation with its pop-up window: just right of the page, its
/// top level with the top of the annotation.
pub(crate) fn finish(out: &mut Built, ctx: &Context<'_>, next_id: &mut dyn FnMut() -> ObjectId, id: ObjectId, mut dict: Dictionary, top: f64) {
    let popup_id = next_id();
    let mut popup = Dictionary::new();
    popup.set("Type", Object::Name(b"Annot".to_vec()));
    popup.set("Subtype", Object::Name(b"Popup".to_vec()));
    popup.set("Parent", Object::Reference(id));
    popup.set(
        "Rect",
        numbers(&[ctx.page_right, top - POPUP_SIZE.1, ctx.page_right + POPUP_SIZE.0, top]),
    );
    popup.set("F", Object::Integer(28));
    popup.set("Open", Object::Boolean(false));
    dict.set("Popup", Object::Reference(popup_id));
    out.objects.push((id, Object::Dictionary(dict)));
    out.objects.push((popup_id, Object::Dictionary(popup)));
    out.annots.push(id);
    out.annots.push(popup_id);
}

pub(crate) fn set_appearance(dict: &mut Dictionary, normal: ObjectId) {
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(normal));
    dict.set("AP", Object::Dictionary(ap));
}

/// The rectangle around quadrilaterals, widened by (dx, dy) for each from
/// its height.
fn around(quads: &[Corners], margin: impl Fn(f64) -> (f64, f64)) -> [f64; 4] {
    let mut r = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for q in quads {
        let height = ((q[0].0 - q[2].0).powi(2) + (q[0].1 - q[2].1).powi(2)).sqrt();
        let (dx, dy) = margin(height);
        for &(x, y) in q {
            r = [r[0].min(x - dx), r[1].min(y - dy), r[2].max(x + dx), r[3].max(y + dy)];
        }
    }
    r
}
