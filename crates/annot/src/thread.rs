//! Grouping annotations into the rows of the comment list.
//!
//! - An annotation with /IRT and /RT /Group belongs to the group of its target
//!   (Acrobat's "replace text" is a StrikeOut grouped under a Caret).
//! - A Text annotation with a /StateModel records a state of its target. Acrobat
//!   chains review states (each new one has /IRT to the previous state), so the
//!   target is found by following /IRT to the start of the thread. A state
//!   whose target is gone is not a row.
//! - Any other annotation with /IRT is a reply; replies to replies join the same thread.
//! - Popups are never rows.

use crate::model::{AnnotKey, Annotation, ReplyType, Subtype};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Highlight,
    Replace,
    Delete,
    Underline,
    Squiggly,
    Insert,
    Note,
    Area,
    Circle,
    Line,
    Polygon,
    PolyLine,
    Ink,
    FreeText,
    Stamp,
    Attachment,
    Sound,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Highlight => "ハイライト",
            Kind::Replace => "置換",
            Kind::Delete => "取り消し線",
            Kind::Underline => "下線",
            Kind::Squiggly => "波線",
            Kind::Insert => "挿入",
            Kind::Note => "メモ",
            Kind::Area => "長方形",
            Kind::Circle => "楕円",
            Kind::Line => "線",
            Kind::Polygon => "多角形",
            Kind::PolyLine => "折れ線",
            Kind::Ink => "手書き",
            Kind::FreeText => "テキストボックス",
            Kind::Stamp => "スタンプ",
            Kind::Attachment => "添付ファイル",
            Kind::Sound => "音声",
            Kind::Other => "その他",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Thread {
    pub kind: Kind,
    /// The annotation that carries the comment (for a replacement, the Caret).
    pub head: usize,
    /// Other annotations of the same group, in file order.
    pub members: Vec<usize>,
    /// Replies, including replies to replies, oldest first.
    pub replies: Vec<usize>,
    /// State records (Marked, Review, ...), oldest first.
    pub states: Vec<usize>,
}

/// Indices refer to the `annots` slice given to `build`.
pub fn build(annots: &[Annotation]) -> Vec<Thread> {
    let index: HashMap<AnnotKey, usize> =
        annots.iter().enumerate().map(|(i, a)| (a.key, i)).collect();
    let target = |i: usize| annots[i].in_reply_to.and_then(|k| index.get(&k).copied());

    // Follows /IRT links up to the annotation that starts the thread.
    let root = |start: usize| {
        let mut cur = start;
        for _ in 0..annots.len() {
            match target(cur) {
                Some(next) if next != start => cur = next,
                _ => break,
            }
        }
        cur
    };

    enum Role {
        Head,
        Member(usize),
        Reply(usize),
        State(usize),
        Skip,
    }

    let roles: Vec<Role> = (0..annots.len())
        .map(|i| {
            let a = &annots[i];
            if !a.subtype.is_markup() {
                return Role::Skip;
            }
            let Some(t) = target(i) else {
                return if a.is_state() { Role::Skip } else { Role::Head };
            };
            let r = root(t);
            if r == i {
                return if a.is_state() { Role::Skip } else { Role::Head };
            }
            if a.is_state() {
                Role::State(r)
            } else if a.reply_type == Some(ReplyType::Group) {
                Role::Member(r)
            } else {
                Role::Reply(r)
            }
        })
        .collect();

    let mut threads: Vec<Thread> = Vec::new();
    let mut thread_of: HashMap<usize, usize> = HashMap::new();
    for (i, role) in roles.iter().enumerate() {
        if matches!(role, Role::Head) {
            thread_of.insert(i, threads.len());
            threads.push(Thread {
                kind: Kind::Other,
                head: i,
                members: Vec::new(),
                replies: Vec::new(),
                states: Vec::new(),
            });
        }
    }
    for (i, role) in roles.iter().enumerate() {
        let (r, list): (usize, fn(&mut Thread) -> &mut Vec<usize>) = match role {
            Role::Member(r) => (*r, |t| &mut t.members),
            Role::Reply(r) => (*r, |t| &mut t.replies),
            Role::State(r) => (*r, |t| &mut t.states),
            _ => continue,
        };
        if let Some(&t) = thread_of.get(&r) {
            list(&mut threads[t]).push(i);
        }
    }
    for t in &mut threads {
        let by_date = |&i: &usize| (annots[i].created.or(annots[i].modified), i);
        t.replies.sort_by_key(by_date);
        t.states.sort_by_key(|&i| (annots[i].date(), i));
        t.kind = kind_of(annots, t);
    }
    threads
}

fn kind_of(annots: &[Annotation], t: &Thread) -> Kind {
    let head = &annots[t.head];
    let has = |s: Subtype| head.subtype == s || t.members.iter().any(|&m| annots[m].subtype == s);
    if has(Subtype::Caret) && has(Subtype::StrikeOut) {
        return Kind::Replace;
    }
    match head.subtype {
        Subtype::Caret if head.intent.as_deref() == Some("Replace") => Kind::Replace,
        Subtype::Caret => Kind::Insert,
        Subtype::StrikeOut => Kind::Delete,
        Subtype::Highlight => Kind::Highlight,
        Subtype::Underline => Kind::Underline,
        Subtype::Squiggly => Kind::Squiggly,
        Subtype::Text => Kind::Note,
        Subtype::Square => Kind::Area,
        Subtype::Circle => Kind::Circle,
        Subtype::Line => Kind::Line,
        Subtype::Polygon => Kind::Polygon,
        Subtype::PolyLine => Kind::PolyLine,
        Subtype::Ink => Kind::Ink,
        Subtype::FreeText => Kind::FreeText,
        Subtype::Stamp => Kind::Stamp,
        Subtype::FileAttachment => Kind::Attachment,
        Subtype::Sound => Kind::Sound,
        _ => Kind::Other,
    }
}

/// Notes that Acrobat shows only in the comment list, never on the page:
/// replies and state records. Indices into `annots`, ascending.
pub fn off_page(annots: &[Annotation], threads: &[Thread]) -> Vec<usize> {
    let mut out: Vec<usize> = threads.iter().flat_map(|t| t.replies.iter().copied()).collect();
    out.extend(annots.iter().enumerate().filter(|(_, a)| a.is_state()).map(|(i, _)| i));
    out.retain(|&i| annots[i].subtype == Subtype::Text);
    out.sort_unstable();
    out.dedup();
    out
}

/// The latest state of `model` recorded on a thread, with the annotation that recorded it.
pub fn latest_state<'a>(
    annots: &'a [Annotation],
    thread: &Thread,
    model: &str,
) -> Option<&'a Annotation> {
    thread
        .states
        .iter()
        .map(|&i| &annots[i])
        .filter(|a| state_model_of(a) == model)
        .max_by_key(|a| a.date())
}

/// Splits the comment of a replacement or insertion into the text to put into
/// the manuscript (first line) and an optional instruction (remaining lines).
/// Acrobat separates lines with CR; LF and CRLF are accepted too.
pub fn split_edit_text(contents: &str) -> (&str, Option<&str>) {
    match contents.find(['\r', '\n']) {
        None => (contents, None),
        Some(end) => {
            let rest = &contents[end..];
            let rest = rest
                .strip_prefix("\r\n")
                .or_else(|| rest.strip_prefix('\r'))
                .or_else(|| rest.strip_prefix('\n'))
                .unwrap_or(rest);
            let rest = rest.trim_matches(['\r', '\n']);
            (&contents[..end], (!rest.is_empty()).then_some(rest))
        }
    }
}

/// /StateModel, inferred from /State when missing (ISO 32000-1, table 172).
pub fn state_model_of(a: &Annotation) -> &str {
    if let Some(m) = a.state_model.as_deref() {
        return m;
    }
    match a.state.as_deref() {
        Some("Marked" | "Unmarked") => "Marked",
        _ => "Review",
    }
}
