//! Text models of real pages through PDFium: the synthetic fixture, and the
//! Acrobat-annotated PDFs in `ref/` when that folder is present.
//! PDFium can be bound once per process, so everything shares one engine.

#[path = "pdf/drag.rs"]
mod drag;

use render::{ChainedBytes, Document, Engine};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use textmodel::{InputChar, PageFrame, Rect, SourceChar, TextModel, inputs_from_source};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn library_dir() -> Option<PathBuf> {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win-x64/bin",
        ("windows", "aarch64") => "win-arm64/bin",
        ("macos", "aarch64") => "mac-arm64/lib",
        ("macos", _) => "mac-x64/lib",
        _ => return None,
    };
    let dir = repo().join("vendor/pdfium").join(target);
    dir.is_dir().then_some(dir)
}

fn source_chars(doc: &Document<'_>, page: usize) -> Vec<SourceChar> {
    doc.chars(page)
        .unwrap()
        .into_iter()
        .map(|c| SourceChar {
            ch: c.ch,
            bbox: c.loose,
            size: c.font_size,
            angle: c.angle,
            origin: c.origin,
            index: c.index,
            generated: c.generated,
        })
        .collect()
}

fn page_input(doc: &Document<'_>, page: usize) -> (Vec<InputChar>, PageFrame) {
    let g = doc.geometry(page).unwrap();
    let frame = PageFrame {
        bbox: Rect::from_array(g.bbox),
        rotation: g.rotation,
    };
    (inputs_from_source(source_chars(doc, page)), frame)
}

fn model(doc: &Document<'_>, page: usize) -> TextModel {
    let g = doc.geometry(page).unwrap();
    let frame = PageFrame {
        bbox: Rect::from_array(g.bbox),
        rotation: g.rotation,
    };
    TextModel::build(&inputs_from_source(source_chars(doc, page)), frame)
}

fn open<'a>(engine: &'a Engine, path: &Path) -> (Document<'a>, Vec<u8>) {
    let bytes = std::fs::read(path).unwrap();
    let doc = engine.open(ChainedBytes::new(vec![Arc::new(bytes.clone())])).unwrap();
    (doc, bytes)
}

#[test]
fn pdf_pages() {
    let Some(dir) = library_dir() else {
        eprintln!("PDFium not found under vendor/pdfium; skipping");
        return;
    };
    let engine = Engine::load(&dir).unwrap();
    fixture_reading_order(&engine);
    ref_markup_targets(&engine);
    drags(&engine, &repo().join("tests/fixtures"), &repo().join("tests/fixtures/selection-cases.json"));
    // Cases drawn from ref/, kept with the working files since the papers are unpublished.
    drags(&engine, &repo().join("ref"), &repo().join("work/selection-cases.json"));
}

/// Drags from `cases` (see `drag::DragCase`) over the PDFs in `dir`.
fn drags(engine: &Engine, dir: &Path, cases: &Path) {
    let Ok(text) = std::fs::read_to_string(cases) else {
        eprintln!("{} not found; skipping", cases.display());
        return;
    };
    if !dir.is_dir() {
        eprintln!("{} not found; skipping", dir.display());
        return;
    }
    let cases: Vec<drag::DragCase> = serde_json::from_str(&text).unwrap();
    let failures = drag::check(engine, dir, &cases);
    eprintln!("{} drags from {}: {} failed", cases.len(), dir.display(), failures.len());
    assert!(failures.is_empty(), "
{}", failures.join("
"));
}

fn fixture_reading_order(engine: &Engine) {
    let (doc, _) = open(engine, &repo().join("tests/fixtures/abstract-2col.pdf"));
    let m = model(&doc, 0);
    let lines: Vec<String> = m.lines.iter().map(|l| m.text(l.chars.clone())).collect();
    let pos = |needle: &str| {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line contains {needle:?}: {lines:#?}"))
    };
    // Running header, one-column title block, left column, right column, page number.
    assert!(pos("講演論文集") < pos("二段組の講演要旨"));
    assert!(pos("山田 太郎") < pos("1. 緒言"));
    assert!(pos("1. 緒言") < pos("囲んで示している"));
    assert!(pos("囲んで示している") < pos("図1"));
    assert!(pos("図1") < pos("3. 結果"));
    assert!(pos("3. 結果") < pos("次のページへ続く"));
    assert_eq!(lines.last().unwrap(), "1");
    // Sub- and superscripts stay on their line.
    assert!(lines.iter().any(|l| l == "試行の総回数は Nα = 180 とし，"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "文字の高さの基準として HtCAP を用いる．"), "{lines:#?}");
    assert!(lines.iter().any(|l| l.starts_with("最大の成功率は Lw* = 0.5")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.starts_with("These results agree with previous test")));

    let m2 = model(&doc, 1);
    let lines2: Vec<String> = m2.lines.iter().map(|l| m2.text(l.chars.clone())).collect();
    assert!(lines2[0].contains("講演論文集"), "{lines2:#?}");
    assert!(lines2[1].starts_with("続きの文章である"), "{lines2:#?}");
    assert_eq!(lines2.last().unwrap(), "2");
}

/// For every text-markup annotation in ref/: when a glyph's ink is covered by
/// one of its quadrilaterals, the text model must report that glyph.
fn ref_markup_targets(engine: &Engine) {
    let Ok(entries) = std::fs::read_dir(repo().join("ref")) else {
        eprintln!("ref/ not found; skipping");
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    let mut checked = 0;
    for path in files {
        let (doc, bytes) = open(engine, &path);
        let file = pdf_io::PdfFile::open(bytes).unwrap();
        let annots = annot::read_all(&file);
        let mut models = std::collections::HashMap::new();
        for a in annots.iter().filter(|a| !a.quads.is_empty()) {
            let m = models.entry(a.page).or_insert_with(|| model(&doc, a.page));
            let quads: Vec<[(f32, f32); 4]> =
                a.quads.iter().map(|q| q.points.map(|(x, y)| (x as f32, y as f32))).collect();
            let covered = source_chars(&doc, a.page).iter().any(|c| {
                if c.generated || c.bbox[2] <= c.bbox[0] {
                    return false;
                }
                let inked = Rect::new(c.bbox[0], c.origin.1, c.bbox[2], c.origin.1 + 0.5 * c.size);
                a.quads.iter().any(|q| {
                    let b = q.bounds();
                    let r = Rect::new(b[0] as f32, b[1] as f32, b[2] as f32, b[3] as f32);
                    r.horizontal_overlap(&inked) >= 0.6 * inked.width() && r.vertical_overlap(&inked) >= 0.6 * inked.height()
                })
            });
            if covered {
                checked += 1;
                assert!(
                    !m.chars_in_quads(&quads).is_empty(),
                    "{}: annotation {} on page {} covers glyphs but none were found",
                    path.display(),
                    a.key,
                    a.page + 1
                );
            }
        }
    }
    eprintln!("checked {checked} annotations in ref/");
}
