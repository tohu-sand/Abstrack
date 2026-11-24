//! `--revisions`: what each saved revision of a file added, changed or removed.
//!
//! The file is cut after every `%%EOF`; each prefix is read as a document and
//! compared object by object with the previous one.

use pdf_io::lopdf::{Object, Writer};
use pdf_io::{ObjectId, PdfFile};
use std::collections::BTreeMap;

pub fn print_revisions(bytes: &[u8]) {
    let ends = eof_offsets(bytes);
    println!("revisions: {}", ends.len());
    let mut previous: Option<BTreeMap<ObjectId, (String, Vec<u8>)>> = None;
    for (n, &end) in ends.iter().enumerate() {
        let prefix = bytes[..end].to_vec();
        let file = match PdfFile::open(prefix) {
            Ok(f) => f,
            Err(e) => {
                println!("\n-- revision {} (ends at {end}): not readable on its own: {e}", n + 1);
                continue;
            }
        };
        let current: BTreeMap<ObjectId, (String, Vec<u8>)> = file
            .doc
            .objects
            .iter()
            .map(|(&id, obj)| (id, (describe(obj), serialize(obj))))
            .collect();
        println!(
            "\n-- revision {} (ends at {end}, {} objects, xref {:?})",
            n + 1,
            current.len(),
            file.xref_kind
        );
        if let Some(prev) = &previous {
            for (id, (what, body)) in &current {
                match prev.get(id) {
                    None => println!("   + {} {}  {what}", id.0, id.1),
                    Some((_, old)) if old != body => println!("   ~ {} {}  {what}", id.0, id.1),
                    _ => {}
                }
            }
            for (id, (what, _)) in prev {
                if !current.contains_key(id) {
                    println!("   - {} {}  {what}", id.0, id.1);
                }
            }
        }
        previous = Some(current);
    }
}

fn eof_offsets(bytes: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(pos) = bytes[i..].windows(5).position(|w| w == b"%%EOF") {
        let mut end = i + pos + 5;
        while end < bytes.len() && (bytes[end] == b'\r' || bytes[end] == b'\n') {
            end += 1;
        }
        out.push(end);
        i = end;
    }
    out
}

fn describe(obj: &Object) -> String {
    let dict = match obj {
        Object::Dictionary(d) => d,
        Object::Stream(s) => &s.dict,
        Object::Array(_) => return "array".into(),
        _ => return String::new(),
    };
    let name = |k: &[u8]| {
        dict.get(k)
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
    };
    match (name(b"Type"), name(b"Subtype")) {
        (Some(t), Some(s)) => format!("/{t} /{s}"),
        (Some(t), None) => format!("/{t}"),
        (None, Some(s)) => format!("/{s}"),
        (None, None) if matches!(obj, Object::Stream(_)) => "stream".into(),
        (None, None) => "dictionary".into(),
    }
}

fn serialize(obj: &Object) -> Vec<u8> {
    let mut out = Vec::new();
    match obj {
        Object::Stream(s) => {
            let _ = Writer::write_object(&mut out, &Object::Dictionary(s.dict.clone()));
            out.extend_from_slice(&s.content);
        }
        other => {
            let _ = Writer::write_object(&mut out, other);
        }
    }
    out
}
