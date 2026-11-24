//! New comments written into the synthetic fixture and read back: the model
//! read from the saved file matches what was asked for (self round trip),
//! the original bytes stay a prefix, and qpdf finds no errors.

use annot::write::{self, Built, CaretMark, Colors, Context, NewComment, with_contents};
use annot::{AnnotKey, Kind, PdfDate, Subtype, build_threads, mark, read_all, reshape};
use pdf_io::{Changes, Object, ObjectId, PdfFile, increment};
use std::path::PathBuf;
use std::sync::Arc;

fn fixture() -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/abstract-2col.pdf")).unwrap()
}

fn line(x0: f64, x1: f64, bottom: f64, top: f64) -> [(f64, f64); 4] {
    [(x0, top), (x1, top), (x0, bottom), (x1, bottom)]
}

const AUTHOR: &str = "添削者 A";
const NOW: PdfDate = PdfDate { year: 2026, month: 11, day: 22, hour: 9, minute: 30, second: 5, offset_minutes: Some(540) };

/// Applies new comments to the file as the application does: the objects go
/// into the document and the changes; the page's /Annots grows.
struct Session {
    file: PdfFile,
    changes: Changes,
    next: u32,
}

impl Session {
    fn new(bytes: Vec<u8>) -> Session {
        let file = PdfFile::open(bytes).unwrap();
        let next = file.doc.trailer.get(b"Size").unwrap().as_i64().unwrap() as u32;
        Session { file, changes: Changes::new(), next }
    }

    fn put(&mut self, id: ObjectId, object: Object) {
        self.file.doc.objects.insert(id, object.clone());
        self.changes.insert(id, Some(object));
    }

    fn add(&mut self, page: usize, comment: NewComment, contents: &str) -> ObjectId {
        self.insert(page, AUTHOR, |ctx, next| write::build(&comment, contents, ctx, next))
    }

    /// A check mark by `author` on the annotation `target`.
    fn check(&mut self, page: usize, target: ObjectId, author: &str) -> ObjectId {
        self.insert(page, author, |ctx, next| mark::new_mark(ctx, target, next))
    }

    /// Puts what `make` builds on a page and returns the annotation that carries the text.
    fn insert(
        &mut self,
        page: usize,
        author: &str,
        make: impl FnOnce(&Context<'_>, &mut dyn FnMut() -> ObjectId) -> Built,
    ) -> ObjectId {
        let page_id = self.file.page_ids()[page];
        let ctx = Context { page: page_id, page_right: 595.2756, author, now: NOW, colors: Colors::default() };
        let mut next = self.next;
        let built = make(&ctx, &mut || {
            next += 1;
            (next - 1, 0)
        });
        self.next = next;
        for (id, object) in built.objects {
            self.put(id, object);
        }
        let (id, object) = write::change_annots(&self.file.doc, page_id, &built.annots, true).unwrap();
        self.put(id, object);
        built.main
    }

    /// Writes the objects `make` gives, which may ask for new object numbers.
    fn apply(&mut self, make: impl FnOnce(&PdfFile, &mut dyn FnMut() -> ObjectId) -> Vec<(ObjectId, Object)>) {
        let mut next = self.next;
        let objects = make(&self.file, &mut || {
            next += 1;
            (next - 1, 0)
        });
        self.next = next;
        for (id, object) in objects {
            self.put(id, object);
        }
    }

    fn save(&mut self) -> Vec<u8> {
        let inc = increment(&self.file, &self.changes).unwrap();
        let mut all = self.file.bytes().as_ref().clone();
        all.extend_from_slice(&inc.bytes);
        self.file.commit(Arc::new(all.clone()), inc.size);
        self.changes.clear();
        all
    }
}

fn qpdf_check(bytes: &[u8], name: &str) {
    let known = PathBuf::from(r"C:\Program Files\qpdf 12.4.2\bin\qpdf.exe");
    let Some(qpdf) = Some(known).filter(|p| p.is_file()) else {
        eprintln!("qpdf not found; skipping the check of {name}");
        return;
    };
    let path = std::env::temp_dir().join(format!("abstrack-write-{}-{name}.pdf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let out = std::process::Command::new(qpdf).arg("--check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "qpdf --check {name}:\n{text}");
}

#[test]
fn every_kind_reads_back_as_written() {
    let original = fixture();
    let mut s = Session::new(original.clone());
    let quads = vec![line(100.0, 200.0, 600.0, 612.5), line(50.0, 120.0, 585.0, 597.5)];
    let caret = CaretMark { x: 150.0, bottom: 550.0, height: 12.5 };
    let japanese = "置換後の文字列\n2行目：全角ＡＢＣと半角abc、記号①～\u{2013}&<>";
    let h = s.add(0, NewComment::Highlight { quads: quads.clone() }, "ハイライトの指摘");
    let d = s.add(0, NewComment::StrikeOut { quads: vec![quads[0]] }, "");
    let u = s.add(0, NewComment::Underline { quads: vec![quads[1]] }, "下線の指摘");
    let r = s.add(0, NewComment::Replace { quads: vec![quads[0]], caret }, japanese);
    let i = s.add(0, NewComment::Insert { caret }, "挿入する語");
    let n = s.add(1, NewComment::Note { at: (80.0, 700.0) }, "図の位置へのメモ");
    let a = s.add(1, NewComment::Area { rect: [300.0, 600.0, 450.0, 700.0] }, "範囲へのコメント");
    let target = [100.0, 597.0, 200.0, 612.5];
    let reply = s.add(0, NewComment::Reply { to: h, target }, "返信です");
    let saved = s.save();
    assert!(saved.starts_with(&original), "the original bytes must stay a prefix");
    qpdf_check(&saved, "every-kind");

    let file = PdfFile::open(saved).unwrap();
    let annots = read_all(&file);
    let get = |id: ObjectId| annots.iter().find(|a| a.key == annot::AnnotKey::Object(id)).unwrap_or_else(|| panic!("{id:?} missing"));
    let normalized = japanese.replace('\n', "\r");

    let hl = get(h);
    assert_eq!(hl.subtype, Subtype::Highlight);
    assert_eq!(hl.quads.len(), 2);
    for (read, written) in hl.quads.iter().zip(&quads) {
        for (p, q) in read.points.iter().zip(written) {
            assert!((p.0 - q.0).abs() < 1e-3 && (p.1 - q.1).abs() < 1e-3);
        }
    }
    assert_eq!(hl.color.len(), 3);
    assert!((hl.opacity.unwrap() - 0.399994).abs() < 1e-5);
    assert_eq!(hl.author.as_deref(), Some(AUTHOR));
    assert_eq!(hl.contents.as_deref(), Some("ハイライトの指摘"));
    assert_eq!(hl.subject.as_deref(), Some("ハイライト表示"));
    assert_eq!(hl.created, Some(NOW));
    assert!(hl.popup.is_some());
    assert!(hl.rich_contents.as_deref().unwrap().contains("&#12495;"));

    assert_eq!(get(d).subtype, Subtype::StrikeOut);
    assert_eq!(get(d).contents, None);
    assert_eq!(get(u).subtype, Subtype::Underline);

    let mark = get(r);
    assert_eq!(mark.subtype, Subtype::Caret);
    assert_eq!(mark.intent.as_deref(), Some("Replace"));
    assert_eq!(mark.contents.as_deref(), Some(normalized.as_str()));
    let strike = annots.iter().find(|x| x.in_reply_to == Some(annot::AnnotKey::Object(r))).unwrap();
    assert_eq!(strike.subtype, Subtype::StrikeOut);
    assert_eq!(strike.intent.as_deref(), Some("StrikeOutTextEdit"));
    assert_eq!(strike.reply_type, Some(annot::ReplyType::Group));
    assert_eq!(strike.contents, None);

    assert_eq!(get(i).subtype, Subtype::Caret);
    assert_eq!(get(i).intent, None);
    assert_eq!(get(n).subtype, Subtype::Text);
    assert_eq!(get(n).icon.as_deref(), Some("Comment"));
    assert_eq!(get(a).subtype, Subtype::Square);
    assert_eq!(get(reply).in_reply_to, Some(annot::AnnotKey::Object(h)));

    // Rows as the comment list shows them: the replacement is one row and
    // the reply hangs under the highlight.
    let threads = build_threads(&annots);
    let kinds: Vec<Kind> = threads.iter().map(|t| t.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == Kind::Replace).count(), 1, "{kinds:?}");
    assert_eq!(threads.len(), 7, "{kinds:?}");
    let highlight = threads.iter().find(|t| annots[t.head].key == annot::AnnotKey::Object(h)).unwrap();
    assert_eq!(highlight.replies.len(), 1);
}

#[test]
fn changing_the_text_keeps_the_other_keys() {
    let mut s = Session::new(fixture());
    let h = s.add(0, NewComment::Highlight { quads: vec![line(100.0, 200.0, 600.0, 612.5)] }, "前の本文");
    let mut dict = s.file.doc.get_dictionary(h).unwrap().clone();
    dict.set("Zzz", Object::Integer(7));
    let later = PdfDate { minute: 45, ..NOW };
    let changed = with_contents(&dict, "新しい本文\n2行目", later);
    assert_eq!(changed.get(b"Zzz").unwrap().as_i64().unwrap(), 7);
    s.put(h, Object::Dictionary(changed));
    let saved = s.save();
    let file = PdfFile::open(saved).unwrap();
    let read = read_all(&file).into_iter().find(|a| a.key == annot::AnnotKey::Object(h)).unwrap();
    assert_eq!(read.contents.as_deref(), Some("新しい本文\r2行目"));
    assert_eq!(read.modified, Some(later));
    assert!(read.rich_contents.unwrap().contains("&#26032;&#12375;&#12356;&#26412;&#25991;&#13;2"));
    // Emptying the text removes /Contents and /RC together.
    let dict = file.doc.get_dictionary(h).unwrap();
    let emptied = with_contents(dict, "", later);
    assert!(!emptied.has(b"Contents") && !emptied.has(b"RC"));
}

#[test]
fn check_marks_are_kept_per_person_and_switched_in_place() {
    let original = fixture();
    let mut s = Session::new(original.clone());
    let h = s.add(0, NewComment::Highlight { quads: vec![line(100.0, 200.0, 600.0, 612.5)] }, "指摘");
    let caret = CaretMark { x: 200.0, bottom: 585.0, height: 12.5 };
    let r = s.add(0, NewComment::Replace { quads: vec![line(100.0, 200.0, 585.0, 597.5)], caret }, "置換後");
    let mine = s.check(0, h, AUTHOR);
    s.check(0, r, "別の人");
    let saved = s.save();
    assert!(saved.starts_with(&original), "the original bytes must stay a prefix");
    qpdf_check(&saved, "marks");

    let file = PdfFile::open(saved.clone()).unwrap();
    let annots = read_all(&file);
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 2, "check marks are not rows");
    let thread = |id: ObjectId| threads.iter().find(|t| annots[t.head].key == AnnotKey::Object(id)).unwrap();
    assert!(mark::is_marked(&annots, thread(h), AUTHOR));
    assert!(!mark::is_marked(&annots, thread(r), AUTHOR), "someone else's check is theirs");
    assert!(mark::is_marked(&annots, thread(r), "別の人"));
    let record = file.doc.get_dictionary(mine).unwrap();
    assert_eq!(record.get(b"F").unwrap().as_i64().unwrap(), 30);
    assert_eq!(record.get(b"State").unwrap().as_str().unwrap(), b"Marked");
    assert_eq!(record.get(b"StateModel").unwrap().as_str().unwrap(), b"Marked");
    let read = annots.iter().find(|a| a.key == AnnotKey::Object(mine)).unwrap();
    assert_eq!(read.contents.as_deref(), Some("添削者 A : Marked"));
    assert_eq!(read.in_reply_to, Some(AnnotKey::Object(h)));
    assert!(read.popup.is_some());

    // Taking the check off rewrites the same record, as Acrobat does.
    let later = PdfDate { minute: 40, ..NOW };
    s.apply(|f, _| vec![(mine, Object::Dictionary(mark::switched(f.doc.get_dictionary(mine).unwrap(), AUTHOR, false, later)))]);
    let resaved = s.save();
    assert!(resaved.starts_with(&saved), "the earlier save must stay a prefix");
    qpdf_check(&resaved, "unmark");
    let annots = read_all(&PdfFile::open(resaved).unwrap());
    let threads = build_threads(&annots);
    let highlight = threads.iter().find(|t| annots[t.head].key == AnnotKey::Object(h)).unwrap();
    assert!(!mark::is_marked(&annots, highlight, AUTHOR));
    assert_eq!(highlight.states.len(), 1, "no second record");
    let read = annots.iter().find(|a| a.key == AnnotKey::Object(mine)).unwrap();
    assert_eq!(read.contents.as_deref(), Some("添削者 A : Unmarked"));
    assert_eq!(read.modified, Some(later));
    assert!(read.rich_contents.as_deref().unwrap().ends_with("<p>&#28155;&#21066;&#32773; A : Unmarked</p></body>"));
}

#[test]
fn moving_the_ends_rewrites_only_the_marking() {
    let mut s = Session::new(fixture());
    let first = vec![line(100.0, 200.0, 600.0, 612.5)];
    let h = s.add(0, NewComment::Highlight { quads: first.clone() }, "ハイライトの指摘");
    let r = s.add(0, NewComment::Replace { quads: first, caret: CaretMark { x: 200.0, bottom: 600.0, height: 12.5 } }, "置換後");
    let saved = s.save();
    let before = read_all(&PdfFile::open(saved.clone()).unwrap());
    let strike = match before.iter().find(|a| a.in_reply_to == Some(annot::AnnotKey::Object(r))).unwrap().key {
        annot::AnnotKey::Object(id) => id,
        other => panic!("{other:?}"),
    };

    let wider = vec![line(80.0, 260.0, 600.0, 612.5), line(50.0, 90.0, 585.0, 597.5)];
    let later = PdfDate { minute: 50, ..NOW };
    for id in [h, strike] {
        s.apply(|f, next| reshape::markup(id, f.doc.get_dictionary(id).unwrap(), &wider, later, next).unwrap());
    }
    let moved = CaretMark { x: 90.0, bottom: 585.0, height: 12.5 };
    s.apply(|f, next| reshape::caret(r, f.doc.get_dictionary(r).unwrap(), moved, later, next));
    let resaved = s.save();
    assert!(resaved.starts_with(&saved), "the earlier save must stay a prefix");
    qpdf_check(&resaved, "reshape");

    let file = PdfFile::open(resaved).unwrap();
    let after = read_all(&file);
    let get = |list: &[annot::Annotation], id: ObjectId| list.iter().find(|a| a.key == annot::AnnotKey::Object(id)).unwrap().clone();
    for id in [h, strike] {
        let (old, new) = (get(&before, id), get(&after, id));
        assert_eq!(new.quads.len(), 2);
        for (read, written) in new.quads.iter().zip(&wider) {
            for (p, q) in read.points.iter().zip(written) {
                assert!((p.0 - q.0).abs() < 1e-3 && (p.1 - q.1).abs() < 1e-3);
            }
        }
        assert!(new.rect[0] <= 50.0 && new.rect[2] >= 260.0 && new.rect[1] <= 585.0 && new.rect[3] >= 612.5, "{:?}", new.rect);
        assert_eq!(new.modified, Some(later));
        assert_eq!(
            (&new.contents, &new.author, new.created, &new.name, new.popup, &new.color, new.opacity, &new.intent, new.in_reply_to),
            (&old.contents, &old.author, old.created, &old.name, old.popup, &old.color, old.opacity, &old.intent, old.in_reply_to),
        );
        let normal = file.doc.get_dictionary(id).unwrap().get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap().as_reference().unwrap();
        assert!(file.doc.get_object(normal).unwrap().as_stream().is_ok());
    }
    let (old, new) = (get(&before, r), get(&after, r));
    assert!(((new.rect[0] + new.rect[2]) / 2.0 - 90.0).abs() < 1e-3, "{:?}", new.rect);
    assert!((new.rect[1] - (585.0 - 0.055 * 12.5)).abs() < 1e-3, "{:?}", new.rect);
    assert_eq!((&new.contents, &new.intent, &new.name), (&old.contents, &old.intent, &old.name));
    let threads = build_threads(&after);
    assert_eq!(threads.len(), 2);
    assert_eq!(threads.iter().filter(|t| t.kind == Kind::Replace).count(), 1);
}
