//! Incremental update: the objects that changed, written after the original
//! bytes, with a cross-reference section of the same kind as the file's last
//! one (a table or a stream) and a trailer that points back to it. The
//! original bytes are left exactly as they are.

use crate::{PdfFile, XrefKind};
use lopdf::{Dictionary, Object, ObjectId, Stream, Writer};
use std::collections::BTreeMap;

/// What one update writes: each object with its new value, or `None` to free
/// its number.
pub type Changes = BTreeMap<ObjectId, Option<Object>>;

#[derive(Debug)]
pub enum UpdateError {
    /// The file is opened for viewing only.
    ReadOnly,
    /// A stream whose content was not kept on loading would be written.
    Hollow(ObjectId),
    /// The trailer has no /Root.
    NoRoot,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::ReadOnly => write!(f, "閲覧専用で開いたファイルには書き込めません"),
            UpdateError::Hollow(id) => {
                write!(f, "読み込み時に中身を保持しなかったオブジェクト {} {} を書き出そうとしました", id.0, id.1)
            }
            UpdateError::NoRoot => write!(f, "文書の /Root が見つかりません"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// The bytes to append to a file, and the /Size its trailer then has.
pub struct Increment {
    pub bytes: Vec<u8>,
    pub size: u32,
}

/// The bytes to append to `file` to record `changes`.
pub fn increment(file: &PdfFile, changes: &Changes) -> Result<Increment, UpdateError> {
    if file.read_only.is_some() {
        return Err(UpdateError::ReadOnly);
    }
    let prev = file.startxref.ok_or(UpdateError::ReadOnly)?;
    let root = file.doc.trailer.get(b"Root").map_err(|_| UpdateError::NoRoot)?.clone();
    let base = file.bytes().len() as u64;
    let mut out: Vec<u8> = Vec::new();
    if !file.bytes().ends_with(b"\n") && !file.bytes().ends_with(b"\r") {
        out.push(b'\n');
    }

    let mut entries: BTreeMap<u32, Entry> = BTreeMap::new();
    for (&id, value) in changes {
        match value {
            Some(object) => {
                if file.hollow.contains(&id) {
                    return Err(UpdateError::Hollow(id));
                }
                entries.insert(id.0, Entry::InUse { offset: base + out.len() as u64, generation: id.1 });
                write_object(&mut out, id, object);
            }
            None => {
                entries.insert(id.0, Entry::Free { generation: id.1.saturating_add(1) });
            }
        }
    }

    let old_size = file
        .doc
        .trailer
        .get(b"Size")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .max(0) as u32;
    let highest = entries.keys().next_back().map_or(0, |&n| n + 1);
    let mut trailer = Dictionary::new();
    trailer.set("Root", root);
    for key in [b"Info".as_slice(), b"ID"] {
        if let Ok(value) = file.doc.trailer.get(key) {
            trailer.set(key, value.clone());
        }
    }
    trailer.set("Prev", Object::Integer(prev as i64));

    match file.xref_kind {
        XrefKind::Table => {
            let start = base + out.len() as u64;
            let size = old_size.max(highest);
            trailer.set("Size", Object::Integer(size as i64));
            out.extend_from_slice(b"xref\n");
            for (first, run) in runs(&entries) {
                out.extend_from_slice(format!("{first} {}\n", run.len()).as_bytes());
                for entry in run {
                    let line = match entry {
                        Entry::InUse { offset, generation } => format!("{offset:010} {generation:05} n\r\n"),
                        Entry::Free { generation } => format!("{:010} {generation:05} f\r\n", 0),
                    };
                    out.extend_from_slice(line.as_bytes());
                }
            }
            out.extend_from_slice(b"trailer\n");
            Writer::write_object(&mut out, &Object::Dictionary(trailer)).expect("writing to memory");
            out.extend_from_slice(format!("\nstartxref\n{start}\n%%EOF\n").as_bytes());
            Ok(Increment { bytes: out, size })
        }
        XrefKind::Stream => {
            // The cross-reference stream is an object too, and lists itself.
            let own = old_size.max(highest);
            let start = base + out.len() as u64;
            entries.insert(own, Entry::InUse { offset: start, generation: 0 });
            let mut rows = Vec::new();
            let mut index = Vec::new();
            for (first, run) in runs(&entries) {
                index.push(Object::Integer(first as i64));
                index.push(Object::Integer(run.len() as i64));
                for entry in run {
                    match entry {
                        Entry::InUse { offset, generation } => {
                            rows.push(1);
                            rows.extend_from_slice(&(*offset as u32).to_be_bytes());
                            rows.extend_from_slice(&generation.to_be_bytes());
                        }
                        Entry::Free { generation } => {
                            rows.push(0);
                            rows.extend_from_slice(&0u32.to_be_bytes());
                            rows.extend_from_slice(&generation.to_be_bytes());
                        }
                    }
                }
            }
            trailer.set("Type", Object::Name(b"XRef".to_vec()));
            trailer.set("Size", Object::Integer(own as i64 + 1));
            trailer.set("Index", Object::Array(index));
            trailer.set(
                "W",
                Object::Array(vec![Object::Integer(1), Object::Integer(4), Object::Integer(2)]),
            );
            let stream = Stream::new(trailer, rows).with_compression(false);
            write_object(&mut out, (own, 0), &Object::Stream(stream));
            out.extend_from_slice(format!("startxref\n{start}\n%%EOF\n").as_bytes());
            Ok(Increment { bytes: out, size: own + 1 })
        }
    }
}

impl PdfFile {
    /// Takes the file with an increment appended as the new base, so that
    /// the next update chains to it. The changed objects are expected to be
    /// in `doc` already.
    pub fn commit(&mut self, bytes: std::sync::Arc<Vec<u8>>, size: u32) {
        self.startxref = crate::tail::find_startxref(&bytes);
        self.bytes = bytes;
        self.doc.trailer.set("Size", Object::Integer(size as i64));
    }
}

#[derive(Debug, Clone, Copy)]
enum Entry {
    InUse { offset: u64, generation: u16 },
    Free { generation: u16 },
}

/// Runs of consecutive object numbers, as cross-reference subsections.
fn runs(entries: &BTreeMap<u32, Entry>) -> Vec<(u32, Vec<&Entry>)> {
    let mut out: Vec<(u32, Vec<&Entry>)> = Vec::new();
    for (&number, entry) in entries {
        match out.last_mut() {
            Some((first, run)) if *first + run.len() as u32 == number => run.push(entry),
            _ => out.push((number, vec![entry])),
        }
    }
    out
}

fn write_object(out: &mut Vec<u8>, id: ObjectId, object: &Object) {
    let before = if Writer::need_separator(object) { " " } else { "" };
    let after = if Writer::need_end_separator(object) { " " } else { "" };
    out.extend_from_slice(format!("{} {} obj\n{before}", id.0, id.1).as_bytes());
    Writer::write_object(out, object).expect("writing to memory");
    out.extend_from_slice(format!("{after}\nendobj\n").as_bytes());
}
