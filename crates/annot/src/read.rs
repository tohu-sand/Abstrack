//! Reading every annotation of every page into `Annotation` values.

use crate::date::PdfDate;
use crate::model::{AnnotKey, Annotation, Quad, ReplyType, Subtype};
use pdf_io::{Dictionary, Object, PdfFile, decode_text};

/// Reads all annotations in page order, then /Annots order within a page.
pub fn read_all(file: &PdfFile) -> Vec<Annotation> {
    let mut out = Vec::new();
    for (page, page_id) in file.page_ids().into_iter().enumerate() {
        for annot in file.page_annots(page_id) {
            let key = match annot.id {
                Some(id) => AnnotKey::Object(id),
                None => AnnotKey::Inline { page, index: annot.index },
            };
            out.push(read_one(file, annot.dict, key, page, annot.index));
        }
    }
    out
}

fn read_one(file: &PdfFile, dict: &Dictionary, key: AnnotKey, page: usize, index: usize) -> Annotation {
    let get = |k: &[u8]| dict.get(k).ok().map(|o| file.resolve(o));
    let text = |k: &[u8]| get(k).and_then(|o| o.as_str().ok()).map(decode_text);
    let name = |k: &[u8]| {
        get(k)
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
    };
    let date = |k: &[u8]| text(k).and_then(|s| PdfDate::parse(&s));
    let reference = |k: &[u8]| match dict.get(k) {
        Ok(Object::Reference(id)) => Some(AnnotKey::Object(*id)),
        _ => None,
    };

    let subtype = name(b"Subtype")
        .map(|n| Subtype::from_name(n.as_bytes()))
        .unwrap_or(Subtype::Other(String::new()));
    let in_reply_to = reference(b"IRT");
    let reply_type = in_reply_to.map(|_| match name(b"RT").as_deref() {
        Some("Group") => ReplyType::Group,
        _ => ReplyType::Reply,
    });

    Annotation {
        key,
        page,
        index_on_page: index,
        subtype,
        intent: name(b"IT"),
        rect: get(b"Rect").map(rect).unwrap_or([0.0; 4]),
        quads: get(b"QuadPoints").map(quads).unwrap_or_default(),
        color: get(b"C").map(numbers).unwrap_or_default(),
        opacity: get(b"CA").and_then(number),
        author: text(b"T"),
        contents: text(b"Contents"),
        rich_contents: get(b"RC").and_then(|o| match o {
            Object::String(bytes, _) => Some(decode_text(bytes)),
            Object::Stream(stream) => stream
                .decompressed_content()
                .ok()
                .map(|c| decode_text(&c)),
            _ => None,
        }),
        subject: text(b"Subj"),
        name: text(b"NM"),
        created: date(b"CreationDate"),
        modified: date(b"M"),
        in_reply_to,
        reply_type,
        state_model: text(b"StateModel").or_else(|| name(b"StateModel")),
        state: text(b"State").or_else(|| name(b"State")),
        popup: reference(b"Popup"),
        parent: reference(b"Parent"),
        icon: name(b"Name"),
    }
}

fn number(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn numbers(o: &Object) -> Vec<f64> {
    o.as_array()
        .map(|items| items.iter().filter_map(number).collect())
        .unwrap_or_default()
}

fn rect(o: &Object) -> [f64; 4] {
    let v = numbers(o);
    if v.len() < 4 {
        return [0.0; 4];
    }
    [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]
}

fn quads(o: &Object) -> Vec<Quad> {
    numbers(o)
        .chunks_exact(8)
        .map(|c| Quad {
            points: [(c[0], c[1]), (c[2], c[3]), (c[4], c[5]), (c[6], c[7])],
        })
        .collect()
}
