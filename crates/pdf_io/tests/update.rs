//! Incremental updates on small files written by lopdf, with a
//! cross-reference table and with a cross-reference stream: the original
//! bytes stay a prefix, a reader sees the new objects, updates chain, and
//! qpdf (when installed) finds no errors.

use pdf_io::lopdf::{self, dictionary};
use pdf_io::{Changes, Object, ObjectId, PdfFile, XrefKind, increment};
use std::path::PathBuf;
use std::sync::Arc;

fn one_page(modern: bool) -> Vec<u8> {
    use lopdf::Document;
    let mut doc = Document::with_version(if modern { "1.7" } else { "1.4" });
    if !modern {
        doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    }
    let pages_id = doc.new_object_id();
    let content = doc.add_object(lopdf::Stream::new(dictionary! {}, b"BT ET".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Resources" => dictionary! {},
        "Contents" => content,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    if modern {
        doc.save_modern(&mut out).unwrap();
    } else {
        doc.save_to(&mut out).unwrap();
    }
    out
}

fn new_id(file: &PdfFile, n: u32) -> ObjectId {
    let size = file.doc.trailer.get(b"Size").unwrap().as_i64().unwrap() as u32;
    (size + n, 0)
}

/// Adds a text note to the first page: the note itself and the page with an /Annots entry.
fn add_note(file: &PdfFile, id: ObjectId, text: &str) -> Changes {
    let page_id = file.page_ids()[0];
    let mut page = file.doc.get_dictionary(page_id).unwrap().clone();
    let mut annots = page.get(b"Annots").ok().and_then(|a| a.as_array().ok().cloned()).unwrap_or_default();
    annots.push(Object::Reference(id));
    page.set("Annots", Object::Array(annots));
    let note = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => vec![100.into(), 700.into(), 124.into(), 724.into()],
        "Contents" => pdf_io::text_object(text),
        "P" => page_id,
    };
    let mut changes = Changes::new();
    changes.insert(id, Some(Object::Dictionary(note)));
    changes.insert(page_id, Some(Object::Dictionary(page)));
    changes
}

fn save(file: &mut PdfFile, changes: &Changes) -> Vec<u8> {
    let inc = increment(file, changes).unwrap();
    let mut all = file.bytes().as_ref().clone();
    all.extend_from_slice(&inc.bytes);
    // Keep the in-memory objects in step, as the application does.
    for (id, value) in changes {
        match value {
            Some(object) => {
                file.doc.objects.insert(*id, object.clone());
            }
            None => {
                file.doc.objects.remove(id);
            }
        }
    }
    file.commit(Arc::new(all.clone()), inc.size);
    all
}

fn notes(bytes: &[u8]) -> Vec<String> {
    let file = PdfFile::open(bytes.to_vec()).unwrap();
    assert!(file.read_only.is_none(), "{:?}", file.read_only);
    file.page_annots(file.page_ids()[0])
        .iter()
        .map(|a| pdf_io::decode_text(a.dict.get(b"Contents").unwrap().as_str().unwrap()))
        .collect()
}

fn qpdf() -> Option<PathBuf> {
    let known = PathBuf::from(r"C:\Program Files\qpdf 12.4.2\bin\qpdf.exe");
    if known.is_file() {
        return Some(known);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|p| p.join(if cfg!(windows) { "qpdf.exe" } else { "qpdf" })).find(|p| p.is_file())
    })
}

/// `qpdf --check` exits with 0 when it finds nothing wrong and 3 for warnings only.
fn check_with_qpdf(bytes: &[u8], name: &str) {
    let Some(qpdf) = qpdf() else {
        eprintln!("qpdf not found; skipping the check of {name}");
        return;
    };
    let path = std::env::temp_dir().join(format!("abstrack-update-{}-{name}.pdf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let out = std::process::Command::new(qpdf).arg("--check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "qpdf --check {name}:\n{text}");
}

fn round_trip(modern: bool) {
    let original = one_page(modern);
    let mut file = PdfFile::open(original.clone()).unwrap();
    assert_eq!(file.xref_kind, if modern { XrefKind::Stream } else { XrefKind::Table });

    let first = new_id(&file, 0);
    let changes = add_note(&file, first, "一つ目：改行\r全角ＡＢＣと半角abc");
    let once = save(&mut file, &changes);
    assert!(once.starts_with(&original), "the original bytes must stay a prefix");
    assert_eq!(notes(&once), vec!["一つ目：改行\r全角ＡＢＣと半角abc"]);
    check_with_qpdf(&once, &format!("once-{modern}"));

    // A second update chains to the first one.
    let second = new_id(&file, 0);
    assert_ne!(second, first);
    let changes = add_note(&file, second, "二つ目");
    let twice = save(&mut file, &changes);
    assert!(twice.starts_with(&once));
    assert_eq!(notes(&twice), vec!["一つ目：改行\r全角ＡＢＣと半角abc", "二つ目"]);
    check_with_qpdf(&twice, &format!("twice-{modern}"));

    // Taking the first note off the page and freeing its number.
    let page_id = file.page_ids()[0];
    let mut page = file.doc.get_dictionary(page_id).unwrap().clone();
    page.set("Annots", Object::Array(vec![Object::Reference(second)]));
    let mut changes = Changes::new();
    changes.insert(page_id, Some(Object::Dictionary(page)));
    changes.insert(first, None);
    let thrice = save(&mut file, &changes);
    assert!(thrice.starts_with(&twice));
    assert_eq!(notes(&thrice), vec!["二つ目"]);
    let reread = PdfFile::open(thrice.clone()).unwrap();
    assert!(reread.doc.get_object(first).is_err(), "the freed object is gone");
    check_with_qpdf(&thrice, &format!("thrice-{modern}"));
}

#[test]
fn update_after_a_cross_reference_table() {
    round_trip(false);
}

#[test]
fn update_after_a_cross_reference_stream() {
    round_trip(true);
}

#[test]
fn hollow_streams_are_never_written() {
    let original = one_page(false);
    let mut file = PdfFile::open(original).unwrap();
    let content = (2, 0);
    file.hollow.insert(content);
    let mut changes = Changes::new();
    changes.insert(content, Some(Object::Null));
    assert!(matches!(increment(&file, &changes), Err(pdf_io::UpdateError::Hollow(_))));
}
