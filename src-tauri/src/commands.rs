//! Functions the window calls.

use crate::annotate::{self, Request};
use crate::bitmaps::{Key, pack, unpack, unpack_into};
use crate::comments::CommentRow;
use crate::document::{DocumentInfo, OutlineItem};
use crate::memory;
use crate::selection::{self, CaretPos, SelectionInfo, Unit};
use crate::open::open_path;
use crate::recovery::{self, Recovery};
use crate::settings::{COLOR_KINDS, color_of, from_hex, to_hex};
use crate::shared::SharedImage;
use crate::state::{AppState, StatusMessage, lock, refresh_menu, save_settings, status};
use crate::texts::{SearchHit, hits_on_page, search_text};
use annot::write::DEFAULT_COLORS;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

/// Largest bitmap side and area the window may ask for.
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 40_000_000;

/// Whether a bitmap of this size is one the window may ask for.
pub fn drawable(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width <= MAX_SIDE && height <= MAX_SIDE && width as u64 * height as u64 <= MAX_PIXELS
}

#[tauri::command]
pub fn document_info(state: State<'_, AppState>) -> Option<DocumentInfo> {
    lock(&state.document).as_ref().map(|d| d.info())
}

/// The rows of the comment list as they are kept, not copied.
pub struct Rows(Arc<Vec<CommentRow>>);

impl Serialize for Rows {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(serializer)
    }
}

/// Off the window's thread, which may be busy handing over page images.
#[tauri::command]
pub async fn comments(app: AppHandle) -> Option<Rows> {
    lock(&app.state::<AppState>().comments).clone().map(Rows)
}

#[tauri::command]
pub fn outline(state: State<'_, AppState>) -> Vec<OutlineItem> {
    lock(&state.document).as_ref().map(|d| d.outline()).unwrap_or_default()
}

#[tauri::command]
pub fn current_status(state: State<'_, AppState>) -> Option<StatusMessage> {
    lock(&state.last_status).clone()
}

#[tauri::command]
pub fn set_visible_pages(state: State<'_, AppState>, pages: Vec<usize>) {
    state.engine.set_wanted(pages);
}

/// The RGBA pixels of a page. With `shared`, they go to the page's script in
/// memory shared with it, announced by that number, and nothing is returned;
/// where that cannot be done, and without it, they are returned, with
/// `compressed` as the deflate stream kept for scrolling back.
#[tauri::command]
pub async fn render_page(
    app: AppHandle,
    generation: u64,
    page: usize,
    width: u32,
    height: u32,
    compressed: Option<bool>,
    shared: Option<u32>,
) -> Result<tauri::ipc::Response, String> {
    if !drawable(width, height) {
        return Err(format!("扱えない大きさです: {width} x {height}"));
    }
    let handle = app.clone();
    let rgba = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let doc = lock(&state.document).clone().filter(|d| d.generation == generation).ok_or(crate::engine::STALE)?;
        if !doc.asked.swap(true, Ordering::Relaxed) && page == 0 {
            crate::open::note_first_draw(&handle, &doc, width, height);
        }
        let revision = doc.edit().page_revisions.get(page).copied().unwrap_or(0);
        let key = Key { page, revision, width, height };
        let cached = lock(&doc.bitmaps).get(key);
        if let Some(token) = shared
            && let Some(mut image) = SharedImage::new(&handle, width as usize * height as usize * 4)
        {
            match &cached {
                Some(packed) => unpack_into(packed, image.bytes_mut())?,
                None => state.engine.render_into(generation, page, width, height, image.bytes_mut())?,
            }
            image.post(token)?;
            if cached.is_none() {
                // The window has the page; keeping it for later need not hold up showing it.
                tauri::async_runtime::spawn_blocking(move || match pack(image.bytes()) {
                    Ok(packed) => lock(&doc.bitmaps).put(key, Arc::new(packed)),
                    Err(e) => status(&handle, format!("ページ画像を圧縮できませんでした: {e}"), true),
                });
            }
            return Ok(None);
        }
        if let Some(packed) = cached {
            return if compressed == Some(true) { Ok(Some(packed.bytes.clone())) } else { unpack(&packed).map(Some) };
        }
        let bitmap = state.engine.render(generation, page, width, height, true)?;
        let packed = Arc::new(pack(&bitmap.rgba)?);
        lock(&doc.bitmaps).put(key, packed.clone());
        Ok::<_, String>(Some(if compressed == Some(true) { packed.bytes.clone() } else { bitmap.rgba }))
    })
    .await
    .map_err(|e| e.to_string())??;
    app.state::<memory::Governor>().note_growth();
    Ok(match rgba {
        Some(bytes) => tauri::ipc::Response::new(bytes),
        None => tauri::ipc::Response::new("null".to_string()),
    })
}

/// Part of a page drawn at `width` x `height`: x, y, w, h in its pixels. Where
/// annotations changed, the window draws this over the page it shows.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn render_region(
    app: AppHandle,
    generation: u64,
    page: usize,
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<tauri::ipc::Response, String> {
    if !drawable(width, height) {
        return Err(format!("扱えない大きさです: {width} x {height}"));
    }
    let handle = app.clone();
    let bitmap = tauri::async_runtime::spawn_blocking(move || {
        handle.state::<AppState>().engine.render_region(generation, page, (width, height), [x, y, w, h])
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bitmap.rgba))
}

#[tauri::command]
pub fn recent_files(state: State<'_, AppState>) -> Vec<String> {
    lock(&state.settings).recent_files.clone()
}

#[tauri::command]
pub fn open_file(app: AppHandle, path: String) {
    open_path(&app, &PathBuf::from(path));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSettings {
    list_visible: bool,
    list_width: u32,
    list_font_size: u32,
}

#[tauri::command]
pub fn ui_settings(state: State<'_, AppState>) -> UiSettings {
    let s = lock(&state.settings);
    UiSettings {
        list_visible: s.list_visible,
        list_width: s.list_width,
        list_font_size: s.list_font_size,
    }
}

#[tauri::command]
pub fn set_ui_settings(app: AppHandle, list_visible: bool, list_width: u32, list_font_size: u32) {
    let state = app.state::<AppState>();
    let visibility_changed = {
        let mut s = lock(&state.settings);
        let changed = s.list_visible != list_visible;
        s.list_visible = list_visible;
        s.list_width = list_width.clamp(200, 900);
        s.list_font_size = list_font_size.clamp(10, 24);
        changed
    };
    save_settings(&app);
    if visibility_changed {
        refresh_menu(&app);
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SearchResults {
    Counts(Vec<usize>),
    Hits(Vec<SearchHit>),
}

/// Occurrences of `query` in reading order, or just their count on each page.
/// Pages without a text model yet are read on the way; progress goes to the
/// status bar when that takes a while.
#[tauri::command]
pub async fn search(app: AppHandle, generation: u64, query: String, start: Option<usize>, end: Option<usize>, counts_only: Option<bool>) -> Result<SearchResults, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let doc = lock(&state.document).clone().ok_or("文書が開かれていません")?;
        if doc.generation != generation {
            return Err(crate::engine::STALE.to_string());
        }
        let started = Instant::now();
        let mut last_report = Instant::now();
        let mut found = if counts_only == Some(true) { SearchResults::Counts(Vec::new()) } else { SearchResults::Hits(Vec::new()) };
        let Some(needle) = textmodel::SearchQuery::new(&query) else { return Ok(found) };
        let whole = start.is_none() && end.is_none();
        for page in start.unwrap_or(0)..end.unwrap_or(doc.pages.len()).min(doc.pages.len()) {
            if lock(&state.document).as_ref().is_none_or(|open| open.generation != generation) {
                return Err(crate::engine::STALE.to_string());
            }
            match &mut found {
                SearchResults::Counts(counts) => counts.push(search_text(&state.engine, &doc, page)?.count(&needle)),
                SearchResults::Hits(hits) => hits.extend(hits_on_page(&state.engine, &doc, page, &needle)?),
            }
            if whole && started.elapsed() > Duration::from_millis(100) && last_report.elapsed() > Duration::from_millis(250) {
                status(&app, format!("検索中 {}/{} ページ", page + 1, doc.pages.len()), false);
                last_report = Instant::now();
            }
        }
        if whole && started.elapsed() > Duration::from_millis(100) {
            let n = match &found {
                SearchResults::Counts(counts) => counts.iter().sum(),
                SearchResults::Hits(hits) => hits.len(),
            };
            status(&app, format!("「{query}」が {n} 件見つかりました"), false);
        }
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The page reports whether it has gone without input for its idle period.
#[tauri::command]
pub fn set_page_idle(app: AppHandle, idle: bool) {
    app.state::<memory::Governor>().set_page_idle(idle);
    if let Some(window) = app.get_webview_window("main") {
        memory::update(&app, &window);
    }
}

#[derive(serde::Deserialize)]
pub struct Prefetch {
    page: usize,
    width: u32,
    height: u32,
}

/// Draws pages about to come into view into the compressed cache, behind any
/// drawing for the pages on screen, so that they appear at once.
#[tauri::command]
pub async fn prefetch_pages(app: AppHandle, generation: u64, pages: Vec<Prefetch>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Some(doc) = lock(&state.document).clone().filter(|d| d.generation == generation) else { return Ok(()) };
        for p in pages {
            if !drawable(p.width, p.height) {
                continue;
            }
            let key = Key {
                page: p.page,
                revision: doc.edit().page_revisions.get(p.page).copied().unwrap_or(0),
                width: p.width,
                height: p.height,
            };
            if lock(&doc.bitmaps).contains(key) {
                continue;
            }
            match state.engine.render(generation, p.page, p.width, p.height, false) {
                Ok(bitmap) => {
                    let packed = Arc::new(pack(&bitmap.rgba)?);
                    lock(&doc.bitmaps).put(key, packed);
                }
                Err(e) if e == crate::engine::SKIPPED || e == crate::engine::STALE => {}
                Err(e) => return Err(e),
            }
        }
        app.state::<memory::Governor>().note_growth();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn caret_at(app: AppHandle, generation: u64, page: usize, x: f32, y: f32) -> Result<Option<CaretPos>, String> {
    tauri::async_runtime::spawn_blocking(move || selection::caret_at(&app, generation, page, x, y))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn select_range(
    app: AppHandle,
    generation: u64,
    anchor: CaretPos,
    focus: CaretPos,
) -> Result<SelectionInfo, String> {
    tauri::async_runtime::spawn_blocking(move || selection::selection(&app, generation, anchor, focus))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn unit_at(app: AppHandle, generation: u64, at: CaretPos, unit: Unit) -> Result<(CaretPos, CaretPos), String> {
    tauri::async_runtime::spawn_blocking(move || selection::unit_at(&app, generation, at, unit))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn move_caret(
    app: AppHandle,
    generation: u64,
    at: CaretPos,
    unit: Unit,
    forward: bool,
    after: bool,
) -> Result<CaretPos, String> {
    tauri::async_runtime::spawn_blocking(move || selection::move_caret(&app, generation, at, unit, forward, after))
        .await
        .map_err(|e| e.to_string())?
}

/// Makes a comment from what is selected; the key of its row.
#[tauri::command]
pub async fn create_comment(app: AppHandle, generation: u64, request: Request, contents: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || annotate::create(&app, generation, request, &contents))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_comment_text(app: AppHandle, generation: u64, key: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || annotate::set_text(&app, generation, &key, &text))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_comment(app: AppHandle, generation: u64, key: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || annotate::delete(&app, generation, &key))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reply_to_comment(app: AppHandle, generation: u64, key: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || annotate::reply(&app, generation, &key, &text))
        .await
        .map_err(|e| e.to_string())?
}

/// Checks a comment as dealt with, or takes the check off.
#[tauri::command]
pub async fn set_comment_marked(app: AppHandle, generation: u64, key: String, marked: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::checks::set_marked(&app, generation, &key, marked))
        .await
        .map_err(|e| e.to_string())?
}

/// Saves to the file the document was opened from (Ctrl+S, or the save
/// state in the status bar).
#[tauri::command]
pub async fn save_document(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::save::save(&app, None))
        .await
        .map_err(|e| e.to_string())?
}

/// Asks where to save, then saves there.
#[tauri::command]
pub fn save_document_as(app: AppHandle) {
    crate::save_as_with_dialog(&app);
}

/// Undoes (or redoes) the last change to the comments; what it was.
#[tauri::command]
pub async fn undo_change(app: AppHandle, generation: u64, redo: bool) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || annotate::undo(&app, generation, redo))
        .await
        .map_err(|e| e.to_string())?
}

/// Reads the open file again after another program changed it.
#[tauri::command]
pub fn reload_document(app: AppHandle) {
    crate::open::reload(&app);
}

/// Changes left by runs that ended without saving them, newest first.
#[tauri::command]
pub fn recovery_offers(app: AppHandle) -> Vec<recovery::Offer> {
    app.state::<Recovery>().offers()
}

#[tauri::command]
pub fn restore_recovery(app: AppHandle, id: String) {
    crate::unsaved::confirm(&app, move |app| {
        if let Err(e) = crate::restore::restore(app, &id) {
            status(app, e, true);
        }
    });
}

#[tauri::command]
pub fn discard_recovery(app: AppHandle, id: String) {
    crate::restore::ask_discard(&app, id);
}

/// What a comment marks, when the handles on the page can move its ends.
#[tauri::command]
pub async fn comment_range(app: AppHandle, generation: u64, key: String) -> Result<Option<crate::marked::Marked>, String> {
    tauri::async_runtime::spawn_blocking(move || crate::marked::marked(&app, generation, &key))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reshape_comment(app: AppHandle, generation: u64, key: String, range: annotate::Range) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::marked::reshape(&app, generation, &key, range))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct AuthorName {
    /// As set; empty when the login name is used.
    name: String,
    login: String,
}

/// The name written as the author of new comments.
#[tauri::command]
pub fn author_name(state: State<'_, AppState>) -> AuthorName {
    AuthorName {
        name: lock(&state.settings).author.clone(),
        login: crate::clock::login_name(),
    }
}

/// Sets the name; check marks are counted for the new name from now on.
#[tauri::command]
pub async fn set_author_name(app: AppHandle, name: String) -> Result<(), String> {
    lock(&app.state::<AppState>().settings).author = name.trim().to_string();
    save_settings(&app);
    tauri::async_runtime::spawn_blocking(move || crate::checks::recount(&app))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindColor {
    kind: &'static str,
    /// `#rrggbb`.
    color: String,
    /// Chosen in place of the default.
    changed: bool,
}

/// The color each kind of new comment gets.
#[tauri::command]
pub fn comment_colors(state: State<'_, AppState>) -> Vec<KindColor> {
    let settings = lock(&state.settings);
    let mut colors = settings.colors();
    COLOR_KINDS
        .iter()
        .map(|&kind| KindColor {
            kind,
            color: color_of(&mut colors, kind).map(|c| to_hex(*c)).unwrap_or_default(),
            changed: settings.comment_colors.contains_key(kind),
        })
        .collect()
}

/// Chooses the colors of new comments, `#rrggbb` by kind. The kinds left out,
/// and a color the same as the default, take the default.
#[tauri::command]
pub fn set_comment_colors(app: AppHandle, colors: BTreeMap<String, String>) -> Result<(), String> {
    let mut chosen = BTreeMap::new();
    for (kind, hex) in colors {
        let mut defaults = DEFAULT_COLORS;
        let default = color_of(&mut defaults, &kind).map(|c| to_hex(*c)).ok_or(format!("{kind} は色を選べる種類ではありません"))?;
        let rgb = from_hex(&hex).ok_or(format!("{hex} は色の書き方として読めません"))?;
        if to_hex(rgb) != default {
            chosen.insert(kind, to_hex(rgb));
        }
    }
    lock(&app.state::<AppState>().settings).comment_colors = chosen;
    save_settings(&app);
    Ok(())
}

/// One item of a right-click menu: a menu bar command and the text shown.
#[derive(serde::Deserialize)]
pub struct MenuEntry {
    id: String,
    text: String,
}

/// Shows `entries` as a menu at the pointer; an empty id draws a line.
#[tauri::command]
pub async fn show_context_menu(app: AppHandle, window: tauri::Window, entries: Vec<MenuEntry>) -> Result<(), String> {
    if let Some(e) = entries.iter().find(|e| !e.id.is_empty() && !crate::menu::WINDOW_COMMANDS.contains(&e.id.as_str())) {
        return Err(format!("{} はメニューに無い項目です", e.id));
    }
    let entries: Vec<(String, String)> = entries.into_iter().map(|e| (e.id, e.text)).collect();
    let handle = app.clone();
    // Made and shown on the window's thread in one go: each item made from
    // another thread waits for a round trip to it.
    app.run_on_main_thread(move || {
        if let Err(e) = crate::menu::context(&handle, &entries).and_then(|menu| window.popup_menu(&menu)) {
            status(&handle, format!("メニューを出せませんでした: {e}"), true);
        }
    })
    .map_err(|e| e.to_string())
}
