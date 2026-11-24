//! PDF text strings (ISO 32000-1, 7.9.2.2): UTF-16BE with BOM, UTF-8 with BOM, or PDFDocEncoding.

use lopdf::{Object, StringFormat};

/// Decodes a text string. Malformed UTF-16 or UTF-8 is replaced with U+FFFD
/// instead of failing, so that one broken annotation does not hide the rest.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return char::decode_utf16(units)
            .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
    }
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|&b| pdfdoc_char(b)).collect()
}

/// Encodes as UTF-16BE with a byte order mark, which every reader accepts
/// for any text, including Japanese.
pub fn encode_text(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + text.len() * 2);
    out.extend_from_slice(b"\xFE\xFF");
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

pub fn text_object(text: &str) -> Object {
    Object::String(encode_text(text), StringFormat::Literal)
}

/// A text string as Acrobat writes one: PDFDocEncoding when every character
/// has a code there, otherwise UTF-16BE with a byte order mark.
pub fn text_string(text: &str) -> Object {
    match encode_pdfdoc(text) {
        Some(bytes) => Object::String(bytes, StringFormat::Literal),
        None => Object::String(encode_text(text), StringFormat::Hexadecimal),
    }
}

/// PDFDocEncoding bytes for `text`, if every character has a code. Of the
/// control characters only tab, line feed and carriage return have one.
pub fn encode_pdfdoc(text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' | ' '..='~' => Some(c as u8),
            // Undefined codes decode to U+FFFD, so the search below would find one of them.
            _ if c == char::REPLACEMENT_CHARACTER => None,
            _ => (0x18u8..=0x1F).chain(0x80..=0xFF).find(|&b| pdfdoc_char(b) == c),
        })
        .collect()
}

fn pdfdoc_char(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x2022, 0x2020, 0x2021, 0x2026, 0x2014, 0x2013, 0x0192, 0x2044, 0x2039, 0x203A, 0x2212,
        0x2030, 0x201E, 0x201C, 0x201D, 0x2018, 0x2019, 0x201A, 0x2122, 0xFB01, 0xFB02, 0x0141,
        0x0152, 0x0160, 0x0178, 0x017D, 0x0131, 0x0142, 0x0153, 0x0161, 0x017E, 0xFFFD,
    ];
    const LOW: [u16; 8] = [0x02D8, 0x02C7, 0x02C6, 0x02D9, 0x02DD, 0x02DB, 0x02DA, 0x02DC];
    let code = match b {
        0x18..=0x1F => LOW[(b - 0x18) as usize],
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        0xA0 => 0x20AC,
        0xAD => 0xFFFD,
        _ => b as u16,
    };
    char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_round_trip() {
        let text = "置換：Ht→Ht_CAP\r改行";
        assert_eq!(decode_text(&encode_text(text)), text);
    }

    #[test]
    fn pdfdoc_ascii_and_specials() {
        assert_eq!(decode_text(b"Abc"), "Abc");
        assert_eq!(decode_text(&[0x84, 0x93]), "\u{2014}\u{FB01}");
    }

    #[test]
    fn pdfdoc_when_possible() {
        assert_eq!(encode_pdfdoc("Fig. 1 \u{2013} é\r"), Some(b"Fig. 1 \x85 \xE9\r".to_vec()));
        assert_eq!(encode_pdfdoc("置換"), None);
        assert!(matches!(text_string("abc"), Object::String(_, StringFormat::Literal)));
        assert!(matches!(text_string("和文"), Object::String(_, StringFormat::Hexadecimal)));
        for text in ["Fig. 1 \u{2013} é\r", "和文と ABC\r改行"] {
            let Object::String(bytes, _) = text_string(text) else { unreachable!() };
            assert_eq!(decode_text(&bytes), text);
        }
    }

    #[test]
    fn broken_utf16_is_replaced() {
        assert_eq!(decode_text(b"\xFE\xFF\xD8\x00\x00A"), "\u{FFFD}A");
    }
}
