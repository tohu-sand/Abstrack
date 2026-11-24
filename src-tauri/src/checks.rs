//! Check marks: the person using the application marks a comment as dealt
//! with, in the form Acrobat uses (`annot::mark`). Each change is one step
//! that can be undone.

use crate::annotate::{author, document, find, object_id, publish};
use crate::clock;
use crate::comments::rows;
use crate::state::{AppState, lock, notify, status};
use crate::texts::list_page_text;
use annot::mark::{self, is_marked, mark_of};
use annot::write::{self, Colors, Context};
use pdf_io::{Object, ObjectId};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

/// Checks a comment, or takes the check off. The check mark is the person's
/// own: someone else's does not count, as in Acrobat.
pub fn set_marked(app: &AppHandle, generation: u64, key: &str, marked: bool) -> Result<(), String> {
    let doc = document(app, generation)?;
    let me = author(app);
    let now = clock::now();
    let mut edit = doc.edit();
    let (target, page, record) = {
        let (annots, thread) = find(&edit.annots, &edit.threads, key)?;
        if is_marked(annots, thread, &me) == marked {
            return Ok(());
        }
        let head = &annots[thread.head];
        let record = mark_of(annots, thread, &me).map(object_id).transpose()?;
        (object_id(head)?, head.page, record)
    };
    let objects: Vec<(ObjectId, Option<Object>)> = match record {
        Some(id) => {
            let dict = edit.file.doc.get_dictionary(id).map_err(|e| e.to_string())?;
            vec![(id, Some(Object::Dictionary(mark::switched(dict, &me, marked, now))))]
        }
        None => {
            let page_id = doc.page_ids[page];
            // The record of a check mark is never drawn, so its color is the default.
            let colors = Colors::default();
            let ctx = Context { page: page_id, page_right: doc.pages[page].media[2], author: &me, now, colors };
            let built = mark::new_mark(&ctx, target, &mut || edit.new_id());
            let (annots_id, annots) = write::change_annots(&edit.file.doc, page_id, &built.annots, true)?;
            let mut objects: Vec<_> = built.objects.into_iter().map(|(id, o)| (id, Some(o))).collect();
            objects.push((annots_id, Some(annots)));
            objects
        }
    };
    edit.apply(if marked { "チェック" } else { "チェックの解除" }, objects);
    drop(edit);
    publish(app, &doc)
}

/// Makes the rows again for the name check marks are counted under, after it
/// changed. Rows still being worked out read the name when they are done.
pub fn recount(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(doc) = lock(&state.document).clone() else {
        return;
    };
    if lock(&state.comments).is_none() {
        return;
    }
    let (annots, threads) = {
        let edit = doc.edit();
        (edit.annots.clone(), edit.threads.clone())
    };
    let me = author(app);
    let list = rows(&annots, &threads, &state.places, doc.generation, &me, |page| {
        list_page_text(&state.engine, &doc, page)
            .map_err(|e| status(app, format!("{} ページの文字を読み取れませんでした: {e}", page + 1), true))
            .ok()
    });
    if state.generation.load(Ordering::SeqCst) != doc.generation {
        return;
    }
    *lock(&state.comments) = Some(Arc::new(list));
    notify(app, "comments-changed");
}
