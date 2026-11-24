//! Values of comment fields as Acrobat writes them: the text with its rich
//! form, dates, and the unique name.

use crate::date::PdfDate;
use pdf_io::{Dictionary, Object, text_string};

/// /Contents and /RC for `text`; both are removed for an empty text.
pub(crate) fn set_contents(dict: &mut Dictionary, text: &str) {
    // Acrobat ends lines with a carriage return.
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if text.is_empty() {
        dict.remove(b"Contents");
        dict.remove(b"RC");
    } else {
        dict.set("Contents", text_string(&text));
        dict.set("RC", Object::String(rich_text(&text).into_bytes(), pdf_io::StringFormat::Literal));
    }
}

/// /RC as Acrobat writes it: XHTML in ASCII, with characters beyond ASCII and
/// line ends as numeric character references.
pub fn rich_text(text: &str) -> String {
    xhtml(&format!(
        "<p dir=\"ltr\"><span dir=\"ltr\" style=\"font-size:10.5pt;text-align:left;\
         color:#000000;font-weight:normal;font-style:normal\">{}</span></p>",
        escape(text)
    ))
}

/// /RC of a state record, which Acrobat writes as one plain paragraph.
pub(crate) fn plain_rich_text(text: &str) -> String {
    xhtml(&format!("<p>{}</p>", escape(text)))
}

fn xhtml(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><body xmlns=\"http://www.w3.org/1999/xhtml\" \
         xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\" xfa:APIVersion=\"Abstrack:{}\" \
         xfa:spec=\"2.0.2\" >{body}</body>",
        env!("CARGO_PKG_VERSION")
    )
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            ' '..='~' => out.push(c),
            _ => out.push_str(&format!("&#{};", c as u32)),
        }
    }
    out
}

pub(crate) fn date(d: PdfDate) -> Object {
    Object::String(d.to_pdf_string().into_bytes(), pdf_io::StringFormat::Literal)
}

/// A random UUID in the form Acrobat writes in /NM.
pub(crate) fn uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    // The counter keeps two calls within one clock tick apart.
    let mut state = time ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed) ^ (std::process::id() as u64) << 32;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&next().to_be_bytes());
    bytes[8..].copy_from_slice(&next().to_be_bytes());
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_look_like_acrobats_and_differ() {
        let a = uuid();
        let b = uuid();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn rich_text_escapes() {
        let rc = rich_text("a<b & 和\r");
        assert!(rc.contains(">a&lt;b &amp; &#21644;&#13;</span>"), "{rc}");
        assert!(rc.is_ascii());
        assert!(rc.starts_with("<?xml version=\"1.0\"?><body xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:xfa="), "{rc}");
        assert!(rc.ends_with("xfa:spec=\"2.0.2\" ><p dir=\"ltr\"><span dir=\"ltr\" style=\"font-size:10.5pt;text-align:left;color:#000000;font-weight:normal;font-style:normal\">a&lt;b &amp; &#21644;&#13;</span></p></body>"), "{rc}");
    }

    #[test]
    fn state_rich_text_is_one_plain_paragraph() {
        let rc = plain_rich_text("添削者 : Marked");
        assert!(rc.ends_with("xfa:spec=\"2.0.2\" ><p>&#28155;&#21066;&#32773; : Marked</p></body>"), "{rc}");
    }
}
