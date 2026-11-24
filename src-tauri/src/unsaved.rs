//! Asking before changes that are not saved would be lost: when the window
//! closes, the application quits, the document is closed or another is opened.

use crate::save;
use crate::state::{AppState, lock, status};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogResult};

const SAVE: &str = "保存する";
const DISCARD: &str = "保存しない";
const CANCEL: &str = "キャンセル";

/// Runs `then` at once when nothing is unsaved; otherwise asks whether to
/// save first, and runs it after saving or when told to discard the changes.
pub fn confirm(app: &AppHandle, then: impl FnOnce(&AppHandle) + Send + 'static) {
    let doc = lock(&app.state::<AppState>().document).clone();
    let Some(doc) = doc.filter(|d| d.edit().is_dirty()) else {
        then(app);
        return;
    };
    let handle = app.clone();
    let buttons = MessageDialogButtons::YesNoCancelCustom(SAVE.into(), DISCARD.into(), CANCEL.into());
    crate::ask::warning(app, format!("{} への変更が保存されていません。保存しますか？", doc.file_name()), buttons)
        .show_with_result(move |result| match result {
            MessageDialogResult::Yes => save_then(&handle, then),
            MessageDialogResult::Custom(label) if label == SAVE => save_then(&handle, then),
            MessageDialogResult::No => then(&handle),
            MessageDialogResult::Custom(label) if label == DISCARD => then(&handle),
            _ => {}
        });
}

fn save_then(app: &AppHandle, then: impl FnOnce(&AppHandle)) {
    match save::save(app, None) {
        Ok(()) => then(app),
        Err(e) => status(app, e, true),
    }
}
