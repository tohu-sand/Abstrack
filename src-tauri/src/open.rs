//! Opening and closing a document, and working out the comment list behind it.

use crate::bitmaps::{Key, pack};
use crate::comments::rows;
use crate::document::OpenDocument;
use crate::recovery::Recovery;
use crate::settings::FirstDraw;
use crate::state::{AppState, lock, notify, refresh_menu, save_settings, set_title, status};
use crate::texts::list_page_text;
use crate::watch::Stamp;
use render::ChainedBytes;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Work to do on a document right after it is opened, instead of the
/// comment list being worked out.
pub type Then = Box<dyn FnOnce(&AppHandle, &Arc<OpenDocument>) + Send>;

/// Opens `path` on a worker thread: PDFium and the annotation reader parse
/// the same bytes at the same time.
pub fn open_path(app: &AppHandle, path: &Path) {
    open_path_then(app, path, None);
}

pub fn open_path_then(app: &AppHandle, path: &Path, then: Option<Then>) {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("open".into())
        .spawn(move || open_now(&app, path, then));
    if let Err(e) = spawned {
        eprintln!("cannot start the open thread: {e}");
    }
}

fn open_now(app: &AppHandle, path: PathBuf, then: Option<Then>) {
    let state = app.state::<AppState>();
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    // Taken before reading, so that a change while reading shows as one.
    let known = Stamp::of(&path);
    let bytes = match std::fs::read(&path) {
        Ok(b) => Arc::new(b),
        Err(e) => return not_opened(app, &path, &e.to_string()),
    };
    let engine = state.engine.clone();
    let engine_bytes = ChainedBytes::new(vec![bytes.clone()]);
    let pdfium = std::thread::spawn(move || engine.open(generation, engine_bytes));
    let parsed = OpenDocument::parse(&path, bytes, known, generation);
    let pdfium = pdfium.join().unwrap_or_else(|_| Err("PDFium のスレッドが止まりました".into()));
    let doc = match (parsed, pdfium) {
        (Ok(doc), Ok(_)) => doc,
        (Err(e), _) | (_, Err(e)) => return not_opened(app, &path, &e),
    };
    if state.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let off_page = doc.edit().off_page();
    if let Err(e) = state.engine.set_off_page(generation, off_page) {
        if e != crate::engine::STALE {
            not_opened(app, &path, &e);
        }
        return;
    }
    let name = doc.file_name();
    let read_only = doc.read_only.clone();
    let doc = Arc::new(doc);
    // Before there is a window, nothing can ask for the document yet.
    if app.get_webview_window("main").is_none() {
        draw_ahead(app, &doc);
    }
    // The document this one replaces had its changes saved or given up.
    if let Err(e) = app.state::<Recovery>().forget() {
        status(app, format!("前の文書の復元用データを削除できませんでした: {e}"), true);
    }
    *lock(&state.document) = Some(doc.clone());
    *lock(&state.comments) = None;
    lock(&state.settings).remember(&path);
    save_settings(app);
    refresh_menu(app);
    set_title(app, &crate::state::window_title(&name, false));
    notify(app, "document-changed");
    match read_only {
        Some(reason) => status(app, reason.message(), true),
        None => status(
            app,
            format!("{name} を開きました"),
            false,
        ),
    }
    match then {
        Some(then) => then(app, &doc),
        None => work_out_comments(app, doc),
    }
}

/// Tells why `path` was not opened. At startup the menu was made for the
/// document asked for; it goes back to what is open.
fn not_opened(app: &AppHandle, path: &Path, reason: &str) {
    status(app, format!("{} を開けませんでした: {reason}", path.display()), true);
    refresh_menu(app);
}

/// Draws the first page into the kept drawings, at the size the window asked
/// for the first page of the last document, when the pages are shaped as
/// that document's. While the program starts, the web view takes several
/// times as long to come up as a page to draw, and the window then finds the
/// page ready. A window that has another size by then asks for its own
/// drawing, and one that could not be made here is made, or reported, then.
fn draw_ahead(app: &AppHandle, doc: &OpenDocument) {
    let state = app.state::<AppState>();
    let Some(FirstDraw { page, widest, width, height }) = lock(&state.settings).first_draw else { return };
    if doc.first_page_shape() != Some((page, widest)) || !crate::commands::drawable(width, height) {
        return;
    }
    state.engine.set_wanted([0]);
    if let Ok(bitmap) = state.engine.render(doc.generation, 0, width, height, true)
        && let Ok(packed) = pack(&bitmap.rgba)
    {
        lock(&doc.bitmaps).put(Key { page: 0, revision: 0, width, height }, Arc::new(packed));
    }
}

/// Remembers how large the window asked for the first page of `doc`, for
/// `draw_ahead` the next time the program starts.
pub fn note_first_draw(app: &AppHandle, doc: &OpenDocument, width: u32, height: u32) {
    let Some((page, widest)) = doc.first_page_shape() else { return };
    let asked = Some(FirstDraw { page, widest, width, height });
    let state = app.state::<AppState>();
    let changed = std::mem::replace(&mut lock(&state.settings).first_draw, asked) != asked;
    if changed {
        save_settings(app);
    }
}

/// Reads the text of the pages that carry comments, one page at a time, and
/// makes the rows. Runs behind page drawing; reports progress when it takes a while.
fn work_out_comments(app: &AppHandle, doc: Arc<OpenDocument>) {
    let state = app.state::<AppState>();
    let total = doc.commented_pages().len();
    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut read = 0;
    let (annots, threads) = {
        let edit = doc.edit();
        (edit.annots.clone(), edit.threads.clone())
    };
    let stale = || state.generation.load(Ordering::SeqCst) != doc.generation;
    let me = crate::annotate::author(app);
    let mut list = rows(&annots, &threads, &state.places, doc.generation, &me, |page| {
        if stale() {
            return None;
        }
        read += 1;
        if started.elapsed() > Duration::from_millis(100) && last_report.elapsed() > Duration::from_millis(250) {
            status(app, format!("コメントを読み込んでいます {read}/{total} ページ"), false);
            last_report = Instant::now();
        }
        match list_page_text(&state.engine, &doc, page) {
            Ok(model) => Some(model),
            Err(e) if e == crate::engine::STALE => None,
            Err(e) => {
                status(app, format!("{} ページの文字を読み取れませんでした: {e}", page + 1), true);
                None
            }
        }
    });
    if stale() {
        return;
    }
    // Check marks count for the name set now, which may have changed meanwhile;
    // the places are known by now, so no page is read again.
    let now = crate::annotate::author(app);
    if now != me {
        list = rows(&annots, &threads, &state.places, doc.generation, &now, |_| None);
    }
    *lock(&state.comments) = Some(Arc::new(list));
    app.state::<crate::memory::Governor>().note_growth();
    notify(app, "comments-changed");
    if started.elapsed() > Duration::from_millis(100) {
        status(app, "コメントを読み込みました", false);
    }
}

pub fn close_document(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    if let Err(e) = app.state::<Recovery>().forget() {
        status(app, format!("復元用データを削除できませんでした: {e}"), true);
    }
    *lock(&state.document) = None;
    *lock(&state.comments) = None;
    state.engine.close();
    refresh_menu(app);
    set_title(app, "Abstrack");
    notify(app, "document-changed");
}

/// Reads the open file again, after it was changed by another program;
/// asks first when that would lose changes made here.
pub fn reload(app: &AppHandle) {
    use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogResult};
    const RELOAD: &str = "再読み込み";
    let doc = lock(&app.state::<AppState>().document).clone();
    let Some(doc) = doc else { return };
    let path = doc.path();
    if !doc.edit().is_dirty() {
        open_path(app, &path);
        return;
    }
    let handle = app.clone();
    let text = format!(
        "{} を読み込み直すと、ここで加えてまだ保存していない変更は失われます。\n\n変更を残すときは「キャンセル」を選び、「名前を付けて保存」で別のファイルに保存してください。",
        doc.file_name()
    );
    crate::ask::warning(app, text, MessageDialogButtons::OkCancelCustom(RELOAD.into(), "キャンセル".into()))
        .show_with_result(move |result| match result {
            MessageDialogResult::Ok => open_path(&handle, &path),
            MessageDialogResult::Custom(label) if label == RELOAD => open_path(&handle, &path),
            _ => {}
        });
}
