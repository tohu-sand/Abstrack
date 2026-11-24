//! The open file as it is being changed: the objects the changes touched
//! since the last save, steps to undo and redo, and the annotations read
//! from the result. The changes live in the parsed document; saving writes
//! the touched objects as an incremental update.

use crate::engine::OffPage;
use annot::{AnnotKey, Annotation, Thread, build_threads, read_all};
use pdf_io::{Changes, Dictionary, Increment, Object, ObjectId, PdfFile};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// One undoable change: the values of the objects it changed, before and
/// after (`None`: the object is absent).
pub struct Step {
    pub label: String,
    before: Vec<(ObjectId, Option<Object>)>,
    after: Vec<(ObjectId, Option<Object>)>,
}

/// The keys of an annotation that decide how it is drawn. A change to any
/// other key (its text, dates, pop-up) leaves the page as it looked.
const DRAWN: &[&[u8]] = &[
    b"Subtype", b"Rect", b"AP", b"AS", b"F", b"CA", b"C", b"IC", b"QuadPoints", b"BS", b"Border", b"RD", b"Name",
];

pub struct Edit {
    pub file: PdfFile,
    pub annots: Arc<Vec<Annotation>>,
    pub threads: Arc<Vec<Thread>>,
    /// Objects changed since the last save, with their values then (`None`:
    /// the object did not exist).
    touched: BTreeMap<ObjectId, Option<Object>>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Lowest object number not yet used.
    next_number: u32,
    /// Bumped for a page whenever its annotations are drawn differently, so
    /// that drawings of the page made before are not used.
    pub page_revisions: Vec<u32>,
    /// Where the last change altered how pages look: page and user-space
    /// rectangle, for drawing again only there.
    pub damage: Vec<(usize, [f64; 4])>,
}

impl Edit {
    pub fn new(file: PdfFile, pages: usize) -> Edit {
        let annots = read_all(&file);
        let threads = build_threads(&annots);
        let size = file.doc.trailer.get(b"Size").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0).max(0) as u32;
        let highest = file.doc.objects.keys().next_back().map_or(0, |id| id.0 + 1);
        Edit {
            file,
            annots: Arc::new(annots),
            threads: Arc::new(threads),
            touched: BTreeMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            next_number: size.max(highest),
            page_revisions: vec![0; pages],
            damage: Vec::new(),
        }
    }

    pub fn new_id(&mut self) -> ObjectId {
        self.next_number += 1;
        (self.next_number - 1, 0)
    }

    /// Whether there is anything to save: undoing every change leaves nothing.
    pub fn is_dirty(&self) -> bool {
        self.touched.iter().any(|(id, saved)| self.file.doc.objects.get(id) != saved.as_ref())
    }

    /// What saving writes now: the objects that differ from the last save.
    pub fn changes(&self) -> Changes {
        let mut changes = Changes::new();
        for (id, saved) in &self.touched {
            let now = self.file.doc.objects.get(id);
            if now != saved.as_ref() {
                changes.insert(*id, now.cloned());
            }
        }
        changes
    }

    /// The bytes to append to the file for the current changes.
    pub fn increment(&self) -> Result<Increment, pdf_io::UpdateError> {
        pdf_io::increment(&self.file, &self.changes())
    }

    /// Makes a change of `objects`, undoable as `label`.
    pub fn apply(&mut self, label: impl Into<String>, objects: Vec<(ObjectId, Option<Object>)>) {
        let before = objects.iter().map(|(id, _)| (*id, self.file.doc.objects.get(id).cloned())).collect();
        let step = Step { label: label.into(), before, after: objects };
        self.set(&step.after);
        self.undo.push(step);
        self.redo.clear();
    }

    /// Undoes the last change; its label, if there was one.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.undo.pop()?;
        self.set(&step.before);
        let label = step.label.clone();
        self.redo.push(step);
        Some(label)
    }

    pub fn redo(&mut self) -> Option<String> {
        let step = self.redo.pop()?;
        self.set(&step.after);
        let label = step.label.clone();
        self.undo.push(step);
        Some(label)
    }

    /// The labels of the changes undo and redo would act on.
    pub fn next_steps(&self) -> (Option<&str>, Option<&str>) {
        (self.undo.last().map(|s| s.label.as_str()), self.redo.last().map(|s| s.label.as_str()))
    }

    /// Takes the saved file as the new base: nothing is pending, and undoing
    /// across the save is not offered.
    pub fn saved(&mut self, bytes: Arc<Vec<u8>>, size: u32) {
        self.file.commit(bytes, size);
        self.touched.clear();
        self.undo.clear();
        self.redo.clear();
    }

    /// The notes that are listed but not drawn on the pages, by page and
    /// position in its /Annots (see `annot::off_page`).
    pub fn off_page(&self) -> OffPage {
        let mut out = OffPage::new();
        for i in annot::off_page(&self.annots, &self.threads) {
            let a = &self.annots[i];
            out.entry(a.page).or_default().push(a.index_on_page);
        }
        out
    }

    fn set(&mut self, values: &[(ObjectId, Option<Object>)]) {
        let before = self.looks(values);
        for (id, value) in values {
            if !self.touched.contains_key(id) {
                self.touched.insert(*id, self.file.doc.objects.get(id).cloned());
            }
            match value {
                Some(object) => {
                    self.next_number = self.next_number.max(id.0 + 1);
                    self.file.doc.objects.insert(*id, object.clone());
                }
                None => {
                    self.file.doc.objects.remove(id);
                }
            }
        }
        let annots = read_all(&self.file);
        self.threads = Arc::new(build_threads(&annots));
        self.annots = Arc::new(annots);
        let after = self.looks(values);
        // Where the pages look different: each annotation drawn differently,
        // where it was and where it is.
        self.damage = before
            .into_iter()
            .zip(after)
            .filter(|(b, a)| b != a)
            .flat_map(|(b, a)| b.into_iter().chain(a))
            .map(|look| (look.page, look.rect))
            .collect();
        let mut pages: Vec<usize> = self.damage.iter().map(|&(page, _)| page).collect();
        pages.sort_unstable();
        pages.dedup();
        for page in pages {
            if let Some(revision) = self.page_revisions.get_mut(page) {
                *revision += 1;
            }
        }
    }

    /// How each object of `values` is drawn now; `None` for what no page
    /// draws: anything but an annotation of a page, pop-ups, and the notes
    /// kept off the page.
    fn looks(&self, values: &[(ObjectId, Option<Object>)]) -> Vec<Option<Look>> {
        let off: HashSet<AnnotKey> = annot::off_page(&self.annots, &self.threads)
            .into_iter()
            .map(|i| self.annots[i].key)
            .collect();
        let placed: HashMap<ObjectId, usize> = self
            .annots
            .iter()
            .filter_map(|a| match a.key {
                AnnotKey::Object(id) if !off.contains(&a.key) => Some((id, a.page)),
                _ => None,
            })
            .collect();
        values
            .iter()
            .map(|(id, _)| look(self.file.doc.objects.get(id)?, *placed.get(id)?))
            .collect()
    }
}

/// How an annotation is drawn and where: its page, its rectangle and the keys
/// that decide its look.
#[derive(PartialEq)]
struct Look {
    page: usize,
    rect: [f64; 4],
    keys: Vec<Option<Object>>,
}

fn look(object: &Object, page: usize) -> Option<Look> {
    let dict: &Dictionary = object.as_dict().ok()?;
    if dict.get(b"Subtype").and_then(Object::as_name).ok()? == b"Popup" {
        return None;
    }
    let r: Vec<f64> = dict.get(b"Rect").ok()?.as_array().ok()?.iter().filter_map(|v| v.as_float().ok().map(f64::from)).collect();
    let [x0, y0, x1, y1] = r[..] else { return None };
    Some(Look {
        page,
        rect: [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)],
        keys: DRAWN.iter().map(|k| dict.get(k).ok().cloned()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::Edit;
    use pdf_io::lopdf::dictionary;
    use pdf_io::{Object, ObjectId, PdfFile};
    use std::sync::Arc;

    fn fixture() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/abstract-2col.pdf")).unwrap()
    }

    fn open(bytes: Vec<u8>) -> Edit {
        let file = PdfFile::open_shared(Arc::new(bytes)).unwrap();
        let pages = file.page_ids().len();
        Edit::new(file, pages)
    }

    /// A note added to the first page: the note and the page pointing to it.
    fn add_note(edit: &mut Edit) -> (ObjectId, ObjectId) {
        let note = edit.new_id();
        let page = edit.file.page_ids()[0];
        let mut dict = edit.file.doc.get_dictionary(page).unwrap().clone();
        dict.set("Annots", vec![Object::Reference(note)]);
        let annot = dictionary! {
            "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![0.into(), 0.into(), 20.into(), 20.into()], "P" => page,
        };
        edit.apply("メモ", vec![(note, Some(Object::Dictionary(annot))), (page, Some(Object::Dictionary(dict)))]);
        (note, page)
    }

    #[test]
    fn only_changes_to_how_an_annotation_looks_mark_the_page() {
        let mut edit = open(fixture());
        let (note, _) = add_note(&mut edit);
        assert_eq!(edit.damage, vec![(0, [0.0, 0.0, 20.0, 20.0])]);
        assert_eq!(edit.page_revisions[0], 1);

        let mut dict = edit.file.doc.get_dictionary(note).unwrap().clone();
        dict.set("Contents", pdf_io::text_object("本文だけ"));
        edit.apply("編集", vec![(note, Some(Object::Dictionary(dict.clone())))]);
        assert!(edit.damage.is_empty(), "{:?}", edit.damage);
        assert_eq!(edit.page_revisions[0], 1);

        dict.set("Rect", vec![100.into(), 200.into(), 120.into(), 220.into()]);
        edit.apply("移動", vec![(note, Some(Object::Dictionary(dict)))]);
        assert_eq!(edit.damage, vec![(0, [0.0, 0.0, 20.0, 20.0]), (0, [100.0, 200.0, 120.0, 220.0])]);
        assert_eq!(edit.page_revisions[0], 2);

        edit.undo();
        assert_eq!(edit.damage.len(), 2);
        edit.apply("削除", vec![(note, None)]);
        assert_eq!(edit.damage, vec![(0, [0.0, 0.0, 20.0, 20.0])]);
    }

    #[test]
    fn a_reply_is_kept_off_the_page_and_leaves_it_as_it_looked() {
        let mut edit = open(fixture());
        let (note, page) = add_note(&mut edit);
        let reply = edit.new_id();
        let mut dict = edit.file.doc.get_dictionary(page).unwrap().clone();
        dict.set("Annots", vec![Object::Reference(note), Object::Reference(reply)]);
        let annot = dictionary! {
            "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![0.into(), 0.into(), 20.into(), 20.into()], "P" => page, "IRT" => note,
        };
        edit.apply("返信", vec![(reply, Some(Object::Dictionary(annot))), (page, Some(Object::Dictionary(dict)))]);
        assert!(edit.damage.is_empty(), "{:?}", edit.damage);
        assert_eq!(edit.page_revisions[0], 1);
        assert_eq!(edit.off_page(), [(0, vec![1])].into_iter().collect());

        edit.undo();
        assert!(edit.damage.is_empty(), "{:?}", edit.damage);
        assert!(edit.off_page().is_empty());
    }

    #[test]
    fn undoing_every_change_leaves_nothing_to_save() {
        let mut edit = open(fixture());
        let page_before = edit.file.doc.objects.get(&edit.file.page_ids()[0]).cloned();
        let (note, page) = add_note(&mut edit);
        assert!(edit.is_dirty());
        assert_eq!(edit.changes().keys().copied().collect::<Vec<_>>(), {
            let mut ids = vec![note, page];
            ids.sort();
            ids
        });
        assert_eq!(edit.annots.len(), 1);
        assert_eq!(edit.page_revisions[0], 1);

        assert_eq!(edit.undo().as_deref(), Some("メモ"));
        assert!(!edit.is_dirty());
        assert!(edit.changes().is_empty());
        assert_eq!(edit.file.doc.objects.get(&page).cloned(), page_before);
        assert!(edit.annots.is_empty());
        assert_eq!(edit.next_steps(), (None, Some("メモ")));

        assert_eq!(edit.redo().as_deref(), Some("メモ"));
        assert!(edit.is_dirty());
        assert_eq!(edit.annots.len(), 1);
        assert_eq!(edit.next_steps(), (Some("メモ"), None));
    }

    #[test]
    fn a_new_change_drops_what_could_be_redone() {
        let mut edit = open(fixture());
        add_note(&mut edit);
        edit.undo();
        add_note(&mut edit);
        assert_eq!(edit.next_steps(), (Some("メモ"), None));
        assert!(edit.redo().is_none());
    }

    #[test]
    fn removing_an_object_that_was_saved_is_a_change() {
        let mut edit = open(fixture());
        let info = edit.file.doc.trailer.get(b"Info").and_then(Object::as_reference).unwrap();
        edit.apply("削除", vec![(info, None)]);
        assert_eq!(edit.changes().into_iter().collect::<Vec<_>>(), vec![(info, None)]);
        edit.undo();
        assert!(!edit.is_dirty());
    }

    #[test]
    fn numbers_given_out_stay_above_objects_put_in() {
        let mut edit = open(fixture());
        let high = (edit.next_number + 10, 0);
        edit.apply("復元", vec![(high, Some(Object::Null))]);
        assert_eq!(edit.new_id(), (high.0 + 1, 0));
    }

    #[test]
    fn saving_takes_the_written_file_as_the_new_start() {
        let base = fixture();
        let mut edit = open(base.clone());
        add_note(&mut edit);
        let inc = edit.increment().unwrap();
        let mut saved = base;
        saved.extend_from_slice(&inc.bytes);
        edit.saved(Arc::new(saved.clone()), inc.size);
        assert!(!edit.is_dirty());
        assert_eq!(edit.next_steps(), (None, None));
        assert_eq!(edit.file.bytes().as_slice(), saved.as_slice());
        assert_eq!(edit.annots.len(), 1);
    }
}
