//! The application's dialogs, owned by its window so that the window waits
//! until they are answered and one question cannot be asked twice.

use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, MessageDialogBuilder, MessageDialogButtons, MessageDialogKind};

/// A question with a warning sign, answered with `buttons`.
pub fn warning(app: &AppHandle, text: impl Into<String>, buttons: MessageDialogButtons) -> MessageDialogBuilder<Wry> {
    let builder = app.dialog().message(text).title("Abstrack").kind(MessageDialogKind::Warning).buttons(buttons);
    match app.get_webview_window("main") {
        Some(window) => builder.parent(&window),
        None => builder,
    }
}

/// The system's dialog for choosing a file to open or a place to save.
pub fn files(app: &AppHandle) -> FileDialogBuilder<Wry> {
    let builder = app.dialog().file();
    match app.get_webview_window("main") {
        Some(window) => builder.set_parent(&window),
        None => builder,
    }
}
