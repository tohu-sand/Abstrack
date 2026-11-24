//! Putting back the changes of a copy that a run left behind when it ended
//! without saving them (see `recovery`), or giving them up.

use crate::document::OpenDocument;
use crate::recovery::{Recovery, remove_files};
use crate::state::{notify, status};
use pdf_io::lopdf::xref::XrefEntry;
use pdf_io::{Object, ObjectId, PdfFile};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogResult};

/// Puts the changes of a copy back: onto the file they belong to when it is
/// still as it was read, as changes not saved yet; otherwise the whole copy
/// is saved as a new file the user names.
pub fn restore(app: &AppHandle, id: &str) -> Result<(), String> {
    let (stem, note) = app.state::<Recovery>().left(id)?;
    let copy = std::fs::read(stem.with_extension("pdf")).map_err(|e| format!("復元用データを読めませんでした: {e}"))?;
    let base = note.base_length as usize;
    let original = std::fs::read(&note.path).ok();
    let fits = copy.len() >= base && original.is_some_and(|o| o.as_slice() == &copy[..base]);
    if !fits {
        save_copy_elsewhere(app, &note.path, stem, copy);
        return Ok(());
    }
    let then: crate::open::Then = Box::new(move |app, doc| {
        match put_back(app, doc, copy, base) {
            Ok(()) => match remove_files(&stem) {
                Ok(()) => status(app, format!("{} の保存されなかった変更を戻しました。保存すると反映されます。", doc.file_name()), false),
                Err(e) => status(app, format!("変更は戻しましたが、復元用データ {} を削除できませんでした: {e}", stem.display()), true),
            },
            Err(e) => status(app, format!("保存されなかった変更を戻せませんでした: {e}。復元用データは残してあります。"), true),
        }
        notify(app, "recovery-changed");
    });
    crate::open::open_path_then(app, &note.path, Some(then));
    Ok(())
}

/// Asks before removing a copy, which cannot be undone.
pub fn ask_discard(app: &AppHandle, id: String) {
    const DISCARD: &str = "破棄";
    let offer = app.state::<Recovery>().offers().into_iter().find(|o| o.id == id);
    let Some(offer) = offer else { return };
    let handle = app.clone();
    let text = format!("{} の時点で保存されなかった {} への変更を破棄します。元に戻せません。", offer.written, offer.file_name);
    crate::ask::warning(app, text, MessageDialogButtons::OkCancelCustom(DISCARD.into(), "キャンセル".into()))
        .show_with_result(move |result| {
            let yes = match result {
                MessageDialogResult::Ok => true,
                MessageDialogResult::Custom(label) => label == DISCARD,
                _ => false,
            };
            if !yes {
                return;
            }
            match handle.state::<Recovery>().discard(&id) {
                Ok(()) => status(&handle, format!("{} への保存されなかった変更を破棄しました", offer.file_name), false),
                Err(e) => status(&handle, e, true),
            }
            notify(&handle, "recovery-changed");
        });
}

/// Applies the objects the copy has after `base` bytes to the freshly opened
/// document, as one change that can be undone.
fn put_back(app: &AppHandle, doc: &Arc<OpenDocument>, copy: Vec<u8>, base: usize) -> Result<(), String> {
    let kept = PdfFile::open_shared(Arc::new(copy)).map_err(|e| e.to_string())?;
    let mut edit = doc.edit();
    let objects = changed_objects(&kept, &edit.file, base)?;
    if objects.is_empty() {
        return Err("復元用データに変更が見つかりませんでした".into());
    }
    edit.apply("保存されなかった変更の復元", objects);
    drop(edit);
    crate::annotate::publish(app, doc)
}

/// The objects written or freed after `base` bytes of `kept`, as changes to
/// `file`, which is those bytes.
pub fn changed_objects(kept: &PdfFile, file: &PdfFile, base: usize) -> Result<Vec<(ObjectId, Option<Object>)>, String> {
    let mut objects = Vec::new();
    for (&number, entry) in &kept.doc.reference_table.entries {
        match *entry {
            XrefEntry::Normal { offset, generation } if offset as usize >= base => {
                let id = (number, generation);
                let Some(object) = kept.doc.objects.get(&id) else { continue };
                if is_xref_stream(object) {
                    continue;
                }
                if kept.hollow.contains(&id) {
                    return Err(format!("復元用データの {number} 番のオブジェクトを読み切れませんでした"));
                }
                objects.push((id, Some(object.clone())));
            }
            XrefEntry::Free | XrefEntry::UnusableFree => {
                let freed = file.doc.objects.range((number, 0)..=(number, u16::MAX)).map(|(&id, _)| (id, None));
                objects.extend(freed);
            }
            // The rest are as in `file`: written before `base`, or in an
            // object stream, which appended changes never use.
            _ => {}
        }
    }
    Ok(objects)
}

fn is_xref_stream(object: &Object) -> bool {
    matches!(object, Object::Stream(s) if s.dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"XRef"))
}

/// The file the changes belong to is no longer as it was read: the whole
/// copy is saved as a new file the user names, and opened.
fn save_copy_elsewhere(app: &AppHandle, original: &Path, stem: PathBuf, copy: Vec<u8>) {
    let name = original
        .file_stem()
        .map(|s| format!("{} - 復元.pdf", s.to_string_lossy()))
        .unwrap_or_else(|| "復元.pdf".into());
    let mut dialog = crate::ask::files(app)
        .set_title("元のファイルが変わっているため、戻した内容を別のファイルとして保存します")
        .add_filter("PDF", &["pdf"])
        .set_file_name(name);
    if let Some(dir) = original.parent().filter(|d| d.exists()) {
        dialog = dialog.set_directory(dir);
    }
    let handle = app.clone();
    dialog.save_file(move |picked| {
        let Some(picked) = picked else { return };
        let target = match picked.into_path() {
            Ok(t) => t,
            Err(e) => return status(&handle, format!("保存先の場所を扱えません: {e}"), true),
        };
        if let Err(e) = crate::save::write_in_place(&target, &copy) {
            return status(&handle, format!("戻した内容を保存できませんでした: {e}。復元用データは残してあります。"), true);
        }
        if let Err(e) = remove_files(&stem) {
            status(&handle, format!("復元用データ {} を削除できませんでした: {e}", stem.display()), true);
        }
        notify(&handle, "recovery-changed");
        crate::open::open_path(&handle, &target);
    });
}

#[cfg(test)]
mod tests {
    use super::changed_objects;
    use pdf_io::lopdf::{Document, dictionary};
    use pdf_io::{Changes, Object, PdfFile};
    use std::sync::Arc;

    fn fixture() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/abstract-2col.pdf")).unwrap()
    }

    /// A new object, a changed one and a removed one, appended to `base` as
    /// a save would, are read back from the copy as the same changes.
    fn round_trip(base: Vec<u8>) {
        let file = PdfFile::open_shared(Arc::new(base.clone())).unwrap();
        let size = file.doc.trailer.get(b"Size").unwrap().as_i64().unwrap() as u32;
        let note = (size, 0);
        let page = file.page_ids()[0];
        let info = file.doc.trailer.get(b"Info").and_then(Object::as_reference).unwrap();
        let mut dict = file.doc.get_dictionary(page).unwrap().clone();
        dict.set("Annots", vec![Object::Reference(note)]);
        let annot = dictionary! { "Type" => "Annot", "Subtype" => "Text", "Contents" => pdf_io::text_object("戻す（改行\nあり）") };
        let mut changes = Changes::new();
        changes.insert(note, Some(Object::Dictionary(annot)));
        changes.insert(page, Some(Object::Dictionary(dict)));
        changes.insert(info, None);
        let mut copy = base.clone();
        copy.extend_from_slice(&pdf_io::increment(&file, &changes).unwrap().bytes);
        let kept = PdfFile::open_shared(Arc::new(copy)).unwrap();
        let back: Changes = changed_objects(&kept, &file, base.len()).unwrap().into_iter().collect();
        assert_eq!(back, changes);
    }

    #[test]
    fn changes_come_back_from_a_copy_with_a_table() {
        round_trip(fixture());
    }

    #[test]
    fn changes_come_back_from_a_copy_with_a_stream() {
        let mut doc = Document::load_mem(&fixture()).unwrap();
        let mut modern = Vec::new();
        doc.save_modern(&mut modern).unwrap();
        round_trip(modern);
    }
}
