//! Saving: the changes appended to the file as an incremental update,
//! written to a new file next to it that then takes its place. Nothing is
//! written when the file on disk is no longer the one that was read.

use crate::annotate::publish;
use crate::state::{AppState, lock, refresh_menu, save_settings, status};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Saves to the file it was opened from, or with `target` to another file.
pub fn save(app: &AppHandle, target: Option<PathBuf>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let doc = lock(&state.document).clone().ok_or("文書が開かれていません")?;
    if let Some(reason) = &doc.read_only {
        return Err(reason.message().to_string());
    }
    let path = doc.path();
    let to = target.clone().unwrap_or_else(|| path.clone());
    let elsewhere = to != path;
    let name = file_name(&to);
    let mut edit = doc.edit();
    if !edit.is_dirty() && !elsewhere {
        status(app, "保存する変更はありません", false);
        return Ok(());
    }
    // The file as read must still be on disk: otherwise someone else wrote
    // it meanwhile (Acrobat, a synchronizing folder), and appending to the
    // bytes read before would throw their change away.
    if !elsewhere {
        let same = crate::watch::matches_file(&path, edit.file.bytes()).map_err(|e| format!("{name} を読み直せなかったので、保存していません: {e}"))?;
        if !same {
            return Err(format!(
                "{name} は開いた後に別のアプリで変更されています。上書きせずに止めました。「名前を付けて保存」で別のファイルに保存してください。"
            ));
        }
    }
    let (tail, size) = if edit.is_dirty() {
        let inc = edit.increment().map_err(|e| e.to_string())?;
        (inc.bytes, inc.size)
    } else {
        let size = edit.file.doc.trailer.get(b"Size").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0) as u32;
        (Vec::new(), size)
    };
    let base = edit.file.bytes();
    let bytes = if tail.is_empty() {
        base.clone()
    } else {
        let mut bytes = Vec::with_capacity(base.len() + tail.len());
        bytes.extend_from_slice(base);
        bytes.extend_from_slice(&tail);
        Arc::new(bytes)
    };
    write_in_place(&to, &bytes).map_err(|e| format!("{name} に保存できませんでした: {e}"))?;
    // What is on disk now must be exactly what was meant to be written.
    let same = crate::watch::matches_file(&to, &bytes).map_err(|e| format!("保存した {name} を読み直せませんでした: {e}"))?;
    if !same {
        return Err(format!("保存した {name} を読み直すと、書き込んだ内容と違っていました。ファイルが壊れているおそれがあります。「名前を付けて保存」で別のファイルに保存してください。"));
    }
    edit.saved(bytes, size);
    // Still under the edit, which the watch takes before it compares the file.
    if elsewhere {
        *lock(&doc.path) = to.clone();
    }
    crate::watch::settle(app, &doc, &to);
    drop(edit);
    if elsewhere {
        lock(&state.settings).remember(&to);
        save_settings(app);
    }
    publish(app, &doc)?;
    refresh_menu(app);
    status(app, format!("{name} を保存しました"), false);
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Writes `bytes` to a new file in the same folder and puts it in the place
/// of `target`, so that a failure halfway leaves `target` as it was.
pub fn write_in_place(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = target.parent().ok_or("保存先のフォルダが分かりません")?;
    let temp = dir.join(format!("{}.abstrack-{}.tmp", file_name(target), std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    let result = written.map_err(|e| format!("一時ファイル {} を書けませんでした: {e}", temp.display())).and_then(|_| replace(target, &temp));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Puts `temp` in the place of `target`. On Windows the replaced file keeps
/// its attributes and access rights.
#[cfg(windows)]
fn replace(target: &Path, temp: &Path) -> Result<(), String> {
    use windows::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW};
    use windows::core::HSTRING;
    if !target.exists() {
        return std::fs::rename(temp, target).map_err(|e| e.to_string());
    }
    // SAFETY: the paths are valid wide strings for the duration of the call.
    unsafe {
        ReplaceFileW(
            &HSTRING::from(target.as_os_str()),
            &HSTRING::from(temp.as_os_str()),
            None,
            REPLACEFILE_IGNORE_MERGE_ERRORS,
            None,
            None,
        )
    }
    .map_err(|e| format!("元のファイルと置き換えられませんでした。ほかのアプリで開いていれば、閉じてから保存してください: {}", e.message()))
}

#[cfg(not(windows))]
fn replace(target: &Path, temp: &Path) -> Result<(), String> {
    std::fs::rename(temp, target).map_err(|e| e.to_string())
}
