//! Renders and reads the synthetic fixture through the vendored PDFium.
//! Run `npm run fetch-pdfium` first; without the library the test only reports that.

use render::{ChainedBytes, Engine};
use std::path::PathBuf;
use std::sync::Arc;

fn library_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium");
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win-x64/bin",
        ("windows", "aarch64") => "win-arm64/bin",
        ("macos", "aarch64") => "mac-arm64/lib",
        ("macos", _) => "mac-x64/lib",
        _ => return None,
    };
    let dir = root.join(target);
    dir.is_dir().then_some(dir)
}

#[test]
fn fixture_pages_render_and_report_characters() {
    let Some(dir) = library_dir() else {
        eprintln!("PDFium not found under vendor/pdfium; skipping");
        return;
    };
    let engine = Engine::load(&dir).expect("load PDFium");
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/abstract-2col.pdf"))
        .expect("fixture");

    // The same file split into two buffers must read as one document.
    let (head, tail) = bytes.split_at(bytes.len() / 2);
    let doc = engine
        .open(ChainedBytes::new(vec![Arc::new(head.to_vec()), Arc::new(tail.to_vec())]))
        .expect("open");
    assert_eq!(doc.page_count(), 2);

    let geometry = doc.geometry(0).unwrap();
    assert_eq!(geometry.rotation, 0);
    let (w, h) = geometry.display_size();
    assert!((w - 595.28).abs() < 0.1 && (h - 841.89).abs() < 0.1, "A4 expected, got {w} x {h}");

    let bitmap = doc.render(0, 300, 424, &[]).unwrap();
    assert_eq!(bitmap.rgba.len(), 300 * 424 * 4);
    let dark = bitmap.rgba.chunks_exact(4).filter(|p| p[0] < 128).count();
    assert!(dark > 500, "page 1 should have visible text, found {dark} dark pixels");

    // Memory of the caller's gets the same picture, whatever it held before.
    let mut own = vec![7; bitmap.rgba.len()];
    doc.render_into(0, 300, 424, &[], &mut own).unwrap();
    assert!(own == bitmap.rgba, "drawn in place");
    assert!(doc.render_into(0, 300, 424, &[], &mut own[4..]).is_err(), "a buffer of another size");

    let chars = doc.chars(0).unwrap();
    let text: String = chars.iter().filter(|c| !c.generated).map(|c| c.ch).collect();
    assert!(text.contains("本要旨の記載内容"), "page text: {text}");
    let sub = chars.iter().find(|c| c.ch == 'α').expect("subscript alpha");
    let base = chars.iter().find(|c| c.ch == 'N' && c.origin.1 > sub.origin.1).expect("N of Nα");
    assert!(sub.font_size < base.font_size, "subscript is smaller");

    region_matches_whole(&doc);
    notes_left_out_are_not_drawn(&engine);
}

/// Not a test of its own: PDFium is bound once per process, and tests run side by side.
fn region_matches_whole(doc: &render::Document<'_>) {
    for (width, height) in [(893u32, 1263u32), (2480, 3508)] {
        let whole = doc.render(0, width, height, &[]).unwrap();
        let edge = [width - 77, height - 5, 77, 5];
        for region in [[0u32, 0, 40, 30], [137, 222, 300, 41], [401, 640, 120, 33], [50, 900, 777, 64], edge] {
            let [x, y, w, h] = region;
            let part = doc.render_region(0, (width, height), region, &[]).unwrap();
            assert_eq!((part.width, part.height), (w, h));
            for row in 0..h {
                let from = (((y + row) * width + x) * 4) as usize;
                let expected = &whole.rgba[from..from + (w * 4) as usize];
                let got = &part.rgba[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
                assert!(got == expected, "row {row} of region {region:?} at {width}x{height}");
            }
        }
        assert!(doc.render_region(0, (width, height), [width - 10, 0, 11, 5], &[]).is_err());
    }
}

/// A 100 pt square page whose /Annots holds a null, a note, a square and a
/// second note, each drawn from its appearance as a black box 20 pt wide.
fn page_with_notes() -> Vec<u8> {
    let content = "0 0 0 rg 0 0 20 20 re f";
    let annot = |subtype: &str, x: u32| {
        format!("<< /Type /Annot /Subtype /{subtype} /Rect [{x} 70 {} 90] /F 4 /AP << /N 4 0 R >> >>", x + 20)
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [null 5 0 R 6 0 R 7 0 R] >>".to_string(),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {} >>\nstream\n{content}\nendstream", content.len()),
        annot("Text", 10),
        annot("Square", 40),
        annot("Text", 70),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).into_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").into_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).into_bytes());
    out
}

fn notes_left_out_are_not_drawn(engine: &Engine) {
    let doc = engine.open(ChainedBytes::new(vec![Arc::new(page_with_notes())])).expect("open");
    // Centres of the three boxes at 1 px per point: note, square, note.
    let dark = |bitmap: &render::Bitmap| -> Vec<bool> {
        [20usize, 50, 80].iter().map(|&x| bitmap.rgba[(20 * 100 + x) * 4] < 128).collect()
    };
    assert_eq!(dark(&doc.render(0, 100, 100, &[]).unwrap()), [true, true, true]);
    // Position 2 holds the square, which is drawn all the same; 9 holds nothing.
    assert_eq!(dark(&doc.render(0, 100, 100, &[1, 2, 9]).unwrap()), [false, true, true]);
    let part = doc.render_region(0, (100, 100), [60, 0, 40, 40], &[3]).unwrap();
    assert!(part.rgba[(20 * 40 + 20) * 4] >= 128, "the second note is left out of a region too");
}
