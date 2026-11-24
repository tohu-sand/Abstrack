//! Turning a run of characters back into a string.

use crate::{Char, TextModel};

impl TextModel {
    /// The text of characters `indices` (in reading order, need not be contiguous).
    /// Lines are joined without a break. A space goes where the source marked
    /// a word break, between Latin words split by a line break, and at a wide
    /// gap between characters that are not Japanese.
    pub fn text_of(&self, indices: &[usize]) -> String {
        let mut out = String::new();
        self.visit_text(indices.iter().copied(), |ch, _| out.push(ch));
        out
    }

    /// Like [`TextModel::text_of`], with the source of every character of the
    /// result: the model character it shows, or `None` for an inserted space.
    pub fn text_with_sources(&self, indices: &[usize]) -> (String, Vec<Option<usize>>) {
        let mut out = String::new();
        let mut sources = Vec::with_capacity(indices.len());
        self.visit_text(indices.iter().copied(), |ch, source| {
            out.push(ch);
            sources.push(source);
        });
        (out, sources)
    }

    pub(crate) fn visit_text(&self, indices: impl Iterator<Item = usize>, mut visit: impl FnMut(char, Option<usize>)) {
        let mut prev: Option<&Char> = None;
        for i in indices {
            let Some(c) = self.chars.get(i) else { continue };
            if let Some(p) = prev
                && needs_space(p, c)
            {
                visit(' ', None);
            }
            visit(c.ch, Some(i));
            prev = Some(c);
        }
    }

    pub fn text(&self, range: std::ops::Range<usize>) -> String {
        let mut out = String::new();
        self.append_page_text(range, &mut out);
        out
    }

    /// Appends the selected part of this page to the preceding pages of a selection.
    pub fn append_page_text(&self, range: std::ops::Range<usize>, out: &mut String) {
        let range = range.start..range.end.min(self.chars.len());
        if range.is_empty() {
            return;
        }
        if out.chars().next_back().is_some_and(|c| crate::joins_with_space(c, self.chars[range.start].ch)) {
            out.push(' ');
        }
        self.visit_text(range, |ch, _| out.push(ch));
    }
}

fn needs_space(prev: &Char, next: &Char) -> bool {
    if prev.ch.is_whitespace() || next.ch.is_whitespace() {
        return false;
    }
    if prev.line != next.line {
        return is_latin(prev.ch) && is_latin(next.ch) && prev.ch != '-';
    }
    if next.space_before && next.display.x0 >= prev.display.x1 - 0.1 * next.size {
        return true;
    }
    if is_cjk(prev.ch) || is_cjk(next.ch) {
        return false;
    }
    next.display.x0 - prev.display.x1 > 0.25 * prev.size.max(next.size)
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '\u{00C0}'..='\u{024F}' | '\u{0370}'..='\u{03FF}')
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF00}'..='\u{FFEF}')
}
