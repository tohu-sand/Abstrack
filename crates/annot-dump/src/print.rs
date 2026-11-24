//! Rendering PDF objects as readable PDF-like syntax, with text strings decoded.

use pdf_io::{Dictionary, Object, PdfFile, decode_text, lopdf};
use std::fmt::Write;

pub struct Printer<'a> {
    pub file: &'a PdfFile,
    pub show_streams: bool,
}

impl Printer<'_> {
    pub fn dict_block(&self, dict: &Dictionary, indent: usize, out: &mut String) {
        for (key, value) in dict.iter() {
            let _ = write!(out, "{:indent$}/{} ", "", name(key), indent = indent);
            self.value(key, value, indent, out);
            out.push('\n');
        }
    }

    fn value(&self, key: &[u8], value: &Object, indent: usize, out: &mut String) {
        match (key, value) {
            (b"AP", Object::Dictionary(ap)) => {
                out.push_str("<<\n");
                for (state, entry) in ap.iter() {
                    let _ = write!(out, "{:w$}/{} ", "", name(state), w = indent + 2);
                    self.appearance(entry, indent + 2, out);
                    out.push('\n');
                }
                let _ = write!(out, "{:indent$}>>", "", indent = indent);
            }
            (b"AP", Object::Reference(_)) => {
                let _ = write!(out, "{} -> ", inline(value));
                if let Ok(ap) = self.file.resolve(value).as_dict() {
                    self.value(key, &Object::Dictionary(ap.clone()), indent, out);
                }
            }
            (b"Popup" | b"IRT" | b"Parent", Object::Reference(id)) => {
                let _ = write!(out, "{} 0 R", id.0);
                if id.1 != 0 {
                    let _ = write!(out, " (gen {})", id.1);
                }
                if let Ok(target) = self.file.doc.get_dictionary(*id) {
                    let subtype = target
                        .get(b"Subtype")
                        .ok()
                        .and_then(|o| o.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).into_owned())
                        .unwrap_or_default();
                    let _ = write!(out, "  % -> {subtype}");
                }
            }
            _ => out.push_str(&inline(value)),
        }
    }

    fn appearance(&self, entry: &Object, indent: usize, out: &mut String) {
        match self.file.resolve(entry) {
            Object::Stream(stream) => {
                out.push_str(&inline(entry));
                out.push_str(" stream ");
                out.push_str(&inline(&Object::Dictionary(stream.dict.clone())));
                if self.show_streams {
                    let content = stream
                        .decompressed_content()
                        .unwrap_or_else(|_| stream.content.clone());
                    out.push('\n');
                    for line in String::from_utf8_lossy(&content).lines() {
                        let _ = writeln!(out, "{:w$}| {}", "", line, w = indent + 2);
                    }
                    let _ = write!(out, "{:w$}", "", w = indent);
                }
            }
            Object::Dictionary(states) => {
                out.push_str("<< ");
                for (state, sub) in states.iter() {
                    let _ = write!(out, "/{} ", name(state));
                    self.appearance(sub, indent + 2, out);
                    out.push(' ');
                }
                out.push_str(">>");
            }
            other => out.push_str(&inline(other)),
        }
    }
}

pub fn name(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        if b.is_ascii_graphic() && !b"()<>[]{}/%#".contains(&b) {
            s.push(b as char);
        } else {
            let _ = write!(s, "#{b:02X}");
        }
    }
    s
}

pub fn inline(obj: &Object) -> String {
    let mut out = String::new();
    write_inline(obj, &mut out);
    out
}

fn write_inline(obj: &Object, out: &mut String) {
    match obj {
        Object::Null => out.push_str("null"),
        Object::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Object::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Object::Real(r) => {
            let _ = write!(out, "{r}");
        }
        Object::Name(n) => {
            out.push('/');
            out.push_str(&name(n));
        }
        Object::String(bytes, format) => out.push_str(&string(bytes, *format)),
        Object::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_inline(item, out);
            }
            out.push(']');
        }
        Object::Dictionary(dict) => {
            out.push_str("<<");
            for (k, v) in dict.iter() {
                out.push_str(" /");
                out.push_str(&name(k));
                out.push(' ');
                write_inline(v, out);
            }
            out.push_str(" >>");
        }
        Object::Stream(stream) => {
            write_inline(&Object::Dictionary(stream.dict.clone()), out);
            let _ = write!(out, " stream({} bytes)", stream.content.len());
        }
        Object::Reference(id) => {
            let _ = write!(out, "{} {} R", id.0, id.1);
        }
    }
}

/// Text strings are shown decoded, tagged with their encoding:
/// `u"..."` UTF-16BE, `u8"..."` UTF-8, `"..."` PDFDocEncoding; binary data as `<hex>`.
fn string(bytes: &[u8], format: lopdf::StringFormat) -> String {
    let tag = if bytes.starts_with(b"\xFE\xFF") {
        "u"
    } else if bytes.starts_with(b"\xEF\xBB\xBF") {
        "u8"
    } else {
        ""
    };
    let text = decode_text(bytes);
    let printable = !tag.is_empty()
        || bytes
            .iter()
            .all(|&b| b >= 0x20 || b == b'\r' || b == b'\n' || b == b'\t');
    if !printable {
        let mut hex = String::from("<");
        for b in bytes {
            let _ = write!(hex, "{b:02X}");
        }
        hex.push('>');
        return hex;
    }
    let mut s = String::new();
    s.push_str(tag);
    if format == lopdf::StringFormat::Hexadecimal {
        s.push('h');
    }
    s.push('"');
    for c in text.chars() {
        match c {
            '\r' => s.push_str("\\r"),
            '\n' => s.push_str("\\n"),
            '\t' => s.push_str("\\t"),
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}
