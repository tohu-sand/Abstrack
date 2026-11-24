//! Checks over the Acrobat-annotated PDFs in `ref/`. The folder is not in the
//! repository; without it these tests pass without checking anything.

use annot::{Kind, Subtype, build_threads, read_all};
use pdf_io::PdfFile;
use std::path::PathBuf;

fn ref_pdfs() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("ref/ not found; skipping");
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    files
}

#[test]
fn every_markup_annotation_lands_in_one_thread() {
    for path in ref_pdfs() {
        let file = PdfFile::open(std::fs::read(&path).unwrap()).unwrap();
        let annots = read_all(&file);
        let threads = build_threads(&annots);
        let mut seen = vec![0u32; annots.len()];
        for t in &threads {
            for &i in std::iter::once(&t.head)
                .chain(&t.members)
                .chain(&t.replies)
                .chain(&t.states)
            {
                seen[i] += 1;
            }
        }
        let keys: std::collections::HashSet<_> = annots.iter().map(|a| a.key).collect();
        for (i, a) in annots.iter().enumerate() {
            let orphan_state =
                a.is_state() && a.in_reply_to.is_none_or(|target| !keys.contains(&target));
            let expected = u32::from(a.subtype.is_markup() && !orphan_state);
            assert_eq!(seen[i], expected, "{}: annotation {} ({})", path.display(), a.key, a.subtype.name());
        }
    }
}

#[test]
fn acrobat_replacements_are_single_rows() {
    for path in ref_pdfs() {
        let file = PdfFile::open(std::fs::read(&path).unwrap()).unwrap();
        let annots = read_all(&file);
        let threads = build_threads(&annots);
        for t in &threads {
            for &m in &t.members {
                if annots[m].intent.as_deref() == Some("StrikeOutTextEdit") {
                    assert_eq!(t.kind, Kind::Replace, "{}: {}", path.display(), annots[m].key);
                    assert_eq!(annots[t.head].subtype, Subtype::Caret);
                }
            }
        }
        let grouped_strikeouts = annots
            .iter()
            .filter(|a| a.intent.as_deref() == Some("StrikeOutTextEdit"))
            .count();
        let replace_rows = threads.iter().filter(|t| t.kind == Kind::Replace).count();
        assert_eq!(grouped_strikeouts, replace_rows, "{}", path.display());
    }
}

#[test]
fn popups_point_back_to_their_parent() {
    for path in ref_pdfs() {
        let file = PdfFile::open(std::fs::read(&path).unwrap()).unwrap();
        let annots = read_all(&file);
        for a in annots.iter().filter(|a| a.popup.is_some()) {
            let popup = annots.iter().find(|p| Some(p.key) == a.popup);
            if let Some(p) = popup {
                assert_eq!(p.parent, Some(a.key), "{}: popup of {}", path.display(), a.key);
            }
        }
    }
}

#[test]
fn hollow_loading_reads_the_same_annotations() {
    for path in ref_pdfs() {
        let bytes = std::fs::read(&path).unwrap();
        let full = PdfFile::open(bytes.clone()).unwrap();
        let slim = PdfFile::open_shared(std::sync::Arc::new(bytes)).unwrap();
        let a = read_all(&full);
        let b = read_all(&slim);
        assert_eq!(a.len(), b.len(), "{}", path.display());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.key, y.key);
            assert_eq!(x.contents, y.contents, "{}: {}", path.display(), x.key);
            assert_eq!(x.rich_contents, y.rich_contents, "{}: {}", path.display(), x.key);
            assert_eq!(x.quads, y.quads);
            assert_eq!(x.in_reply_to, y.in_reply_to);
        }
        assert!(!slim.hollow.is_empty() || full.doc.objects.len() < 50, "{}: nothing was dropped", path.display());
    }
}
