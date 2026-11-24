//! What the application holds while it runs, and the messages it sends to the window.

use crate::comments::{CommentRow, Places};
use crate::document::OpenDocument;
use crate::engine::EngineHandle;
use crate::menu;
use crate::settings::Settings;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
    pub document: Mutex<Option<Arc<OpenDocument>>>,
    /// Comment rows of the open document; `None` while they are being worked out.
    pub comments: Mutex<Option<Arc<Vec<CommentRow>>>>,
    pub places: Mutex<Places>,
    pub generation: AtomicU64,
    pub engine: EngineHandle,
    pub settings: Mutex<Settings>,
    /// The menu bar as last built or updated, and its items that change with edits.
    pub menu: Mutex<Option<(menu::MenuState, menu::Live<tauri::Wry>)>>,
    pub settings_path: Option<PathBuf>,
    /// Kept so that a window that starts listening late still shows it.
    pub last_status: Mutex<Option<StatusMessage>>,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
}

pub fn status(app: &AppHandle, text: impl Into<String>, is_error: bool) {
    let message = StatusMessage {
        text: text.into(),
        is_error,
    };
    *lock(&app.state::<AppState>().last_status) = Some(message.clone());
    if let Err(e) = app.emit("status", message) {
        eprintln!("status event failed: {e}");
    }
}

pub fn notify(app: &AppHandle, event: &str) {
    if let Err(e) = app.emit(event, ()) {
        eprintln!("{event} event failed: {e}");
    }
}

pub fn save_settings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(path) = &state.settings_path else {
        return;
    };
    let settings = lock(&state.settings).clone();
    if let Err(e) = settings.save(path) {
        status(app, format!("設定を {} に保存できませんでした: {e}", path.display()), true);
    }
}

/// The menu for the document and the settings as they are.
fn menu_state(app: &AppHandle) -> menu::MenuState {
    let state = app.state::<AppState>();
    let doc = lock(&state.document).clone();
    let (editable, dirty, undo, redo) = match &doc {
        Some(d) => {
            let edit = d.edit();
            let (undo, redo) = edit.next_steps();
            (d.read_only.is_none(), edit.is_dirty(), undo.map(str::to_string), redo.map(str::to_string))
        }
        None => (false, false, None, None),
    };
    let (recent, list_visible) = {
        let s = lock(&state.settings);
        (s.recent_files.clone(), s.list_visible)
    };
    menu::MenuState { recent, list_visible, has_document: doc.is_some(), editable, dirty, undo, redo }
}

pub fn refresh_menu(app: &AppHandle) {
    show_menu(app, menu_state(app));
}

/// The menu as it will be once the document at `path` is open, for the
/// window to be made with. The thread that opens the document then finds the
/// menu as it needs it; making it from there waits for the window's thread
/// at every item, and that thread is busy starting the web view.
pub fn expect_document_menu(app: &AppHandle, path: &Path) {
    let mut settings = lock(&app.state::<AppState>().settings).clone();
    settings.remember(path);
    show_menu(app, menu::MenuState { recent: settings.recent_files, has_document: true, editable: true, ..menu_state(app) });
}

fn show_menu(app: &AppHandle, new: menu::MenuState) {
    let state = app.state::<AppState>();
    let mut current = lock(&state.menu);
    let result = match current.as_ref() {
        Some((old, _)) if *old == new => Ok(()),
        Some((old, live)) if old.same_layout(&new) => live.update(old, &new),
        _ => menu::build(app, &new).and_then(|(m, live)| {
            app.set_menu(m)?;
            *current = Some((new.clone(), live));
            Ok(())
        }),
    };
    match result {
        Ok(()) => {
            if let Some((state, _)) = current.as_mut() {
                *state = new;
            }
        }
        Err(e) => {
            // Built again next time.
            *current = None;
            drop(current);
            status(app, format!("メニューを更新できませんでした: {e}"), true);
        }
    }
}

/// The window's title for a file: marked with * while changes are not saved.
pub fn window_title(file_name: &str, dirty: bool) -> String {
    if dirty { format!("*{file_name} - Abstrack") } else { format!("{file_name} - Abstrack") }
}

pub fn set_title(app: &AppHandle, title: &str) {
    if let Some(window) = app.get_webview_window("main")
        && let Err(e) = window.set_title(title)
    {
        eprintln!("set_title failed: {e}");
    }
}
