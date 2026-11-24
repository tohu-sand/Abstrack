//! Making, changing and removing comments: what the keys of the page area
//! and the comment list do. Each is one step that can be undone.

use crate::clock;
use crate::comments::rows;
use crate::document::OpenDocument;
use crate::recovery::Recovery;
use crate::selection::CaretPos;
use crate::state::{AppState, lock, refresh_menu, set_title, status};
use crate::texts::{list_page_text, page_text};
use annot::write::{self, CaretMark, Colors, Context, NewComment, with_contents};
use annot::{AnnotKey, Annotation, Thread};
use pdf_io::{Object, ObjectId};
use render::ChainedBytes;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use textmodel::{Caret, TextModel, span};

#[derive(Deserialize, Clone, Copy)]
pub struct Range {
    pub anchor: CaretPos,
    pub focus: CaretPos,
}

/// A comment to make, from what is selected on the page.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Request {
    Highlight { ranges: Vec<Range> },
    StrikeOut { ranges: Vec<Range> },
    Underline { ranges: Vec<Range> },
    Replace { ranges: Vec<Range> },
    /// At a caret position; `after` as for `TextModel::line_step`.
    Insert { at: CaretPos, after: bool },
    /// At a point of a page, user space.
    Note { page: usize, x: f64, y: f64 },
    /// On a rectangle of a page, user space.
    Area { page: usize, rect: [f64; 4] },
}

/// Sent to the window after every change.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    pub generation: u64,
    pub dirty: bool,
    pub page_revisions: Vec<u32>,
    /// Where pages look different now: the window draws only there again.
    pub damage: Vec<Damaged>,
}

#[derive(Serialize, Clone)]
pub struct Damaged {
    pub page: usize,
    /// User space.
    pub rect: [f64; 4],
}

pub(crate) fn document(app: &AppHandle, generation: u64) -> Result<Arc<OpenDocument>, String> {
    let doc = lock(&app.state::<AppState>().document).clone().ok_or("文書が開かれていません")?;
    if doc.generation != generation {
        return Err(crate::engine::STALE.to_string());
    }
    if let Some(reason) = &doc.read_only {
        return Err(reason.message().to_string());
    }
    Ok(doc)
}

pub(crate) fn model(app: &AppHandle, doc: &OpenDocument, page: usize) -> Result<Arc<TextModel>, String> {
    let state = app.state::<AppState>();
    page_text(&state.engine, doc, page)
}

/// Makes a comment; the key of its row in the comment list.
pub fn create(app: &AppHandle, generation: u64, request: Request, contents: &str) -> Result<String, String> {
    let doc = document(app, generation)?;
    let (label, shapes) = shapes(app, &doc, request)?;
    let author = author(app);
    let colors = comment_colors(app);
    let now = clock::now();
    let mut edit = doc.edit();
    let mut objects: Vec<(ObjectId, Option<Object>)> = Vec::new();
    let mut main = None;
    for (i, (page, comment)) in shapes.iter().enumerate() {
        let page_id = doc.page_ids[*page];
        let ctx = Context { page: page_id, page_right: doc.pages[*page].media[2], author: &author, now, colors };
        // A comment over two pages is one annotation on each, its text on
        // the first, as Acrobat does.
        let text = if i == 0 { contents } else { "" };
        let built = write::build(comment, text, &ctx, &mut || edit.new_id());
        let (annots_id, annots) = write::change_annots(&edit.file.doc, page_id, &built.annots, true)?;
        objects.extend(built.objects.into_iter().map(|(id, o)| (id, Some(o))));
        objects.push((annots_id, Some(annots)));
        main.get_or_insert(built.main);
    }
    let main = main.ok_or("コメントを入れる文字がありません")?;
    edit.apply(label, objects);
    drop(edit);
    publish(app, &doc)?;
    Ok(AnnotKey::Object(main).to_string())
}

type PageQuads = BTreeMap<usize, Vec<[(f64, f64); 4]>>;

/// What a request marks: a label for undoing it, and a comment per page.
type Shapes = (&'static str, Vec<(usize, NewComment)>);

/// What `request` marks, page by page.
fn shapes(app: &AppHandle, doc: &OpenDocument, request: Request) -> Result<Shapes, String> {
    let quads = |ranges: &[Range]| -> Result<PageQuads, String> {
        let mut by_page: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        for r in ranges {
            let (low, high) = (r.anchor.page.min(r.focus.page), r.anchor.page.max(r.focus.page));
            let mut bodies = HashMap::new();
            for p in low..=high {
                bodies.insert(p, model(app, doc, p)?.page_body());
            }
            for (page, start, end) in span(Caret::from(r.anchor), Caret::from(r.focus), |p| bodies[&p]) {
                by_page.entry(page).or_default().push((start, end));
            }
        }
        let mut out = BTreeMap::new();
        for (page, mut pieces) in by_page {
            pieces.sort_unstable();
            let m = model(app, doc, page)?;
            let q: Vec<[(f64, f64); 4]> = pieces
                .iter()
                .flat_map(|&(s, e)| m.markup_quads(s, e))
                .map(|c| c.map(|(x, y)| (x as f64, y as f64)))
                .collect();
            if !q.is_empty() {
                out.insert(page, q);
            }
        }
        Ok(out)
    };
    Ok(match request {
        Request::Highlight { ranges } => (
            "ハイライト",
            quads(&ranges)?.into_iter().map(|(p, quads)| (p, NewComment::Highlight { quads })).collect(),
        ),
        Request::StrikeOut { ranges } => (
            "取り消し線",
            quads(&ranges)?.into_iter().map(|(p, quads)| (p, NewComment::StrikeOut { quads })).collect(),
        ),
        Request::Underline { ranges } => (
            "下線",
            quads(&ranges)?.into_iter().map(|(p, quads)| (p, NewComment::Underline { quads })).collect(),
        ),
        Request::Replace { ranges } => {
            let by_page = quads(&ranges)?;
            if by_page.len() > 1 {
                return Err("置換はページをまたいで作れません。ページごとに選んでください".into());
            }
            let Some((page, quads)) = by_page.into_iter().next() else {
                return Ok(("置換", Vec::new()));
            };
            // The mark goes after the last character struck out.
            let last = ranges
                .iter()
                .map(|r| if (r.anchor.page, r.anchor.index) > (r.focus.page, r.focus.index) { r.anchor } else { r.focus })
                .max_by_key(|c| (c.page, c.index))
                .expect("a range");
            let caret = mark(&*model(app, doc, page)?, last.index, true)?;
            ("置換", vec![(page, NewComment::Replace { quads, caret })])
        }
        Request::Insert { at, after } => {
            let caret = mark(&*model(app, doc, at.page)?, at.index, after)?;
            ("挿入", vec![(at.page, NewComment::Insert { caret })])
        }
        Request::Note { page, x, y } => ("メモ", vec![(page, NewComment::Note { at: (x, y) })]),
        Request::Area { page, rect } => ("長方形", vec![(page, NewComment::Area { rect })]),
    })
}

pub(crate) fn mark(model: &TextModel, index: usize, after: bool) -> Result<CaretMark, String> {
    let m = model.caret_mark(index, after).ok_or("このページには挿入できる位置がありません")?;
    Ok(CaretMark { x: m.x as f64, bottom: m.bottom as f64, height: m.height as f64 })
}

/// Replaces the text of a comment (the insertion mark's for a replacement).
pub fn set_text(app: &AppHandle, generation: u64, key: &str, text: &str) -> Result<(), String> {
    let doc = document(app, generation)?;
    let mut edit = doc.edit();
    let id = {
        let (annots, thread) = find(&edit.annots, &edit.threads, key)?;
        object_id(&annots[thread.head])?
    };
    let dict = edit.file.doc.get_dictionary(id).map_err(|e| e.to_string())?;
    let current = dict.get(b"Contents").ok().and_then(|o| o.as_str().ok()).map(pdf_io::decode_text).unwrap_or_default();
    if current == text.replace("\r\n", "\r").replace('\n', "\r") {
        return Ok(());
    }
    let changed = with_contents(dict, text, clock::now());
    edit.apply("コメントの編集", vec![(id, Some(Object::Dictionary(changed)))]);
    drop(edit);
    publish(app, &doc)
}

/// Removes a comment with everything that belongs to it: the other half of a
/// replacement, replies, states and pop-up windows.
pub fn delete(app: &AppHandle, generation: u64, key: &str) -> Result<(), String> {
    let doc = document(app, generation)?;
    let mut edit = doc.edit();
    let mut by_page: BTreeMap<usize, Vec<ObjectId>> = BTreeMap::new();
    {
        let (annots, thread) = find(&edit.annots, &edit.threads, key)?;
        let members = std::iter::once(thread.head)
            .chain(thread.members.iter().copied())
            .chain(thread.replies.iter().copied())
            .chain(thread.states.iter().copied());
        for i in members {
            let a = &annots[i];
            let ids = by_page.entry(a.page).or_default();
            ids.push(object_id(a)?);
            if let Some(AnnotKey::Object(popup)) = a.popup {
                ids.push(popup);
            }
        }
    }
    let mut objects = Vec::new();
    for (&page, ids) in &by_page {
        let (annots_id, annots) = write::change_annots(&edit.file.doc, doc.page_ids[page], ids, false)?;
        objects.push((annots_id, Some(annots)));
        objects.extend(ids.iter().map(|&id| (id, None)));
    }
    edit.apply("コメントの削除", objects);
    drop(edit);
    publish(app, &doc)
}

/// Adds a reply to a comment; the key of the comment's row.
pub fn reply(app: &AppHandle, generation: u64, key: &str, text: &str) -> Result<(), String> {
    let doc = document(app, generation)?;
    let author = author(app);
    let mut edit = doc.edit();
    let (target, page, rect) = {
        let (annots, thread) = find(&edit.annots, &edit.threads, key)?;
        let head = &annots[thread.head];
        (object_id(head)?, head.page, head.rect)
    };
    let page_id = doc.page_ids[page];
    let (now, colors) = (clock::now(), comment_colors(app));
    let ctx = Context { page: page_id, page_right: doc.pages[page].media[2], author: &author, now, colors };
    let built = write::build(&NewComment::Reply { to: target, target: rect }, text, &ctx, &mut || edit.new_id());
    let (annots_id, annots) = write::change_annots(&edit.file.doc, page_id, &built.annots, true)?;
    let mut objects: Vec<(ObjectId, Option<Object>)> = built.objects.into_iter().map(|(id, o)| (id, Some(o))).collect();
    objects.push((annots_id, Some(annots)));
    edit.apply("返信", objects);
    drop(edit);
    publish(app, &doc)
}

/// Undoes (or with `redo`, redoes) the last change; what it was.
pub fn undo(app: &AppHandle, generation: u64, redo: bool) -> Result<Option<String>, String> {
    let doc = document(app, generation)?;
    let label = {
        let mut edit = doc.edit();
        if redo { edit.redo() } else { edit.undo() }
    };
    if label.is_some() {
        publish(app, &doc)?;
    }
    Ok(label)
}

pub(crate) fn find<'a>(annots: &'a [Annotation], threads: &'a [Thread], key: &str) -> Result<(&'a [Annotation], &'a Thread), String> {
    threads
        .iter()
        .find(|t| annots[t.head].key.to_string() == key)
        .map(|t| (annots, t))
        .ok_or_else(|| "そのコメントはすでに削除されています".to_string())
}

pub(crate) fn object_id(a: &Annotation) -> Result<ObjectId, String> {
    match a.key {
        AnnotKey::Object(id) => Ok(id),
        AnnotKey::Inline { .. } => Err("このコメントは、変更できない形でファイルに書かれています".into()),
    }
}

/// The name comments and check marks are made under: the one set in the
/// application, or the login name as Acrobat uses by default.
pub(crate) fn author(app: &AppHandle) -> String {
    let name = lock(&app.state::<AppState>().settings).author.clone();
    if name.trim().is_empty() { clock::login_name() } else { name }
}

fn comment_colors(app: &AppHandle) -> Colors {
    lock(&app.state::<AppState>().settings).colors()
}

/// Shows a change: PDFium draws the file with it, the rows are made again,
/// and the window learns which pages to draw again.
pub fn publish(app: &AppHandle, doc: &Arc<OpenDocument>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (bytes, off_page, annots, threads, changed) = {
        let mut edit = doc.edit();
        let damage = std::mem::take(&mut edit.damage).into_iter().map(|(page, rect)| Damaged { page, rect }).collect();
        let base = edit.file.bytes().clone();
        let increment = match edit.is_dirty() {
            true => Some(Arc::new(edit.increment().map_err(|e| e.to_string())?.bytes)),
            false => None,
        };
        app.state::<Recovery>().keep(doc, &base, increment.as_ref());
        let mut parts = vec![base];
        parts.extend(increment);
        let changed = Changed {
            generation: doc.generation,
            dirty: edit.is_dirty(),
            page_revisions: edit.page_revisions.clone(),
            damage,
        };
        (ChainedBytes::new(parts), edit.off_page(), edit.annots.clone(), edit.threads.clone(), changed)
    };
    // Drawing the window asks for once it hears of the change comes after the
    // reload, so the rows need not wait for it: a check mark shows at once
    // even while a page is being drawn.
    let reloaded = state.engine.reload(doc.generation, bytes, off_page);
    let me = author(app);
    let rows = rows(&annots, &threads, &state.places, doc.generation, &me, |page| {
        list_page_text(&state.engine, doc, page)
            .map_err(|e| status(app, format!("{} ページの文字を読み取れませんでした: {e}", page + 1), true))
            .ok()
    });
    *lock(&state.comments) = Some(Arc::new(rows));
    let dirty = changed.dirty;
    app.emit("annotations-changed", changed).map_err(|e| e.to_string())?;
    // The title and the menu wait for the window's thread, which may be busy
    // handing over a page's picture: the rows go first.
    set_title(app, &crate::state::window_title(&doc.file_name(), dirty));
    refresh_menu(app);
    reloaded.recv().map_err(|e| e.to_string())?
}
