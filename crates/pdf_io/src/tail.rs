//! Locating the header and the last cross-reference section in raw bytes.

/// Acrobat accepts the header anywhere in the first 1024 bytes.
pub fn has_pdf_header(bytes: &[u8]) -> bool {
    let window = &bytes[..bytes.len().min(1024)];
    window.windows(5).any(|w| w == b"%PDF-")
}

/// Offset given by the last `startxref` keyword in the final 4096 bytes;
/// `None` when there is none or the offset is past the end of the file.
pub fn find_startxref(bytes: &[u8]) -> Option<u64> {
    let window_start = bytes.len().saturating_sub(4096);
    let tail = &bytes[window_start..];
    let pos = tail.windows(9).rposition(|w| w == b"startxref")?;
    let rest = &tail[pos + 9..];
    let digits: Vec<u8> = rest
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    let offset: u64 = std::str::from_utf8(&digits).ok()?.parse().ok()?;
    if offset as usize >= bytes.len() {
        return None;
    }
    Some(offset)
}

pub fn xref_table_at(bytes: &[u8], offset: u64) -> bool {
    let start = offset as usize;
    let rest = &bytes[start..];
    let trimmed: &[u8] = {
        let skip = rest.iter().take_while(|b| b.is_ascii_whitespace()).count();
        &rest[skip..]
    };
    trimmed.starts_with(b"xref")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_last_startxref() {
        let data = b"%PDF-1.7\n...startxref\n10\n%%EOF\nmore\nstartxref\n 20\r\n%%EOF\n_padding_______";
        assert_eq!(find_startxref(data), Some(20));
    }

    #[test]
    fn rejects_offset_past_end() {
        let data = b"%PDF-1.7\nstartxref\n999999\n%%EOF\n";
        assert_eq!(find_startxref(data), None);
    }

    #[test]
    fn detects_table() {
        let data = b"%PDF-1.4\n\nxref\n0 1\n";
        assert!(xref_table_at(data, 9));
        assert!(!xref_table_at(b"%PDF-1.5\n12 0 obj", 9));
    }
}
