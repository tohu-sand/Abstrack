//! Check marks: whether a person has dealt with a comment, kept the way
//! Acrobat keeps them. A hidden note in reply to the comment records the
//! Marked state. Each person has their own, and Acrobat shows a person only
//! theirs.

use crate::appearance;
use crate::date::PdfDate;
use crate::fields::{date, plain_rich_text};
use crate::model::Annotation;
use crate::thread::{Thread, state_model_of};
use crate::write::{Built, Context, DEFAULT_COLORS, common, finish, set_appearance};
use pdf_io::{Dictionary, Object, ObjectId, StringFormat, text_string};

/// Where Acrobat puts state records, which are never shown.
const RECT: [f64; 4] = [100.0, 76.0, 124.0, 100.0];

/// The latest record of `author`'s check mark on a thread.
pub fn mark_of<'a>(annots: &'a [Annotation], thread: &Thread, author: &str) -> Option<&'a Annotation> {
    thread
        .states
        .iter()
        .map(|&i| &annots[i])
        .filter(|a| state_model_of(a) == "Marked" && a.author.as_deref() == Some(author))
        .max_by_key(|a| a.date())
}

/// Whether `author` has checked the thread.
pub fn is_marked(annots: &[Annotation], thread: &Thread, author: &str) -> bool {
    mark_of(annots, thread, author).is_some_and(|a| a.state.as_deref() == Some("Marked"))
}

/// A check mark by `ctx.author` on the annotation `target`.
pub fn new_mark(ctx: &Context<'_>, target: ObjectId, next_id: &mut dyn FnMut() -> ObjectId) -> Built {
    let mut out = Built { objects: Vec::new(), annots: Vec::new(), main: (0, 0) };
    let id = next_id();
    let mut dict = common(ctx, "Text", "ノート注釈", RECT, DEFAULT_COLORS.note, "");
    dict.set("F", Object::Integer(30));
    dict.set("Name", Object::Name(b"Comment".to_vec()));
    dict.set("IRT", Object::Reference(target));
    dict.set("StateModel", Object::String(b"Marked".to_vec(), StringFormat::Literal));
    set_state(&mut dict, ctx.author, true);
    let ap = next_id();
    out.objects.push((ap, Object::Stream(appearance::note_icon(DEFAULT_COLORS.note))));
    set_appearance(&mut dict, ap);
    finish(&mut out, ctx, next_id, id, dict, RECT[3]);
    out.main = id;
    out
}

/// The record `dict` of `author`'s check mark switched on or off, as Acrobat
/// switches it: /State, /Contents, /RC and /M change, and nothing else.
pub fn switched(dict: &Dictionary, author: &str, marked: bool, now: PdfDate) -> Dictionary {
    let mut out = dict.clone();
    set_state(&mut out, author, marked);
    out.set("M", date(now));
    out
}

fn set_state(dict: &mut Dictionary, author: &str, marked: bool) {
    let state = if marked { "Marked" } else { "Unmarked" };
    let text = format!("{author} : {state}");
    dict.set("State", Object::String(state.as_bytes().to_vec(), StringFormat::Literal));
    dict.set("Contents", text_string(&text));
    dict.set("RC", Object::String(plain_rich_text(&text).into_bytes(), StringFormat::Literal));
}
