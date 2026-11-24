//! Finding a string in the page text, in reading order.

use crate::TextModel;
use std::ops::Range;

/// Folded page text and the byte offsets of word breaks added between glyphs.
pub struct SearchText {
    text: Box<str>,
    spaces: Box<[usize]>,
}

impl SearchText {
    pub fn of(model: &TextModel) -> Self {
        let mut text = String::with_capacity(model.chars.len());
        let mut spaces = Vec::new();
        model.visit_text(0..model.chars.len(), |ch, source| {
            if source.is_none() {
                spaces.push(text.len());
            }
            text.push(fold(ch));
        });
        Self { text: text.into_boxed_str(), spaces: spaces.into_boxed_slice() }
    }

    pub fn byte_len(&self) -> usize {
        self.text.len() + std::mem::size_of_val(&*self.spaces)
    }

    pub fn count(&self, query: &SearchQuery) -> usize {
        self.text.matches(query.text.as_str()).count()
    }

    /// One model character per folded character, except for the inserted spaces.
    pub fn ranges(&self, query: &SearchQuery) -> Vec<Range<usize>> {
        let mut ranges = Vec::new();
        let (mut prev_byte, mut prev_char) = (0, 0);
        for (byte, found) in self.text.match_indices(query.text.as_str()) {
            let start = prev_char + self.text[prev_byte..byte].chars().count();
            let end = start + query.chars;
            prev_byte = byte + found.len();
            prev_char = end;
            let first = start - self.spaces.partition_point(|&s| s < byte);
            let last = end - self.spaces.partition_point(|&s| s < prev_byte);
            ranges.push(first..last);
        }
        ranges
    }
}

pub struct SearchQuery {
    text: String,
    chars: usize,
}

impl SearchQuery {
    /// `None` for a query that is empty or only white space.
    pub fn new(query: &str) -> Option<Self> {
        let text: String = query.chars().map(fold).collect();
        if text.trim().is_empty() {
            return None;
        }
        Some(Self { chars: text.chars().count(), text })
    }
}

/// One occurrence: the characters it covers, in reading order.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub chars: Vec<usize>,
}

impl TextModel {
    /// Occurrences of `query` in the page. Lines are joined as in
    /// [`TextModel::text_of`], so a match can run across a line break.
    /// Letters are compared without case, and full-width forms of ASCII
    /// characters equal their ASCII forms.
    pub fn find(&self, query: &str) -> Vec<Match> {
        let Some(query) = SearchQuery::new(query) else { return Vec::new() };
        SearchText::of(self).ranges(&query).into_iter().map(|range| Match { chars: range.collect() }).collect()
    }
}

/// One character for each character, which [`SearchText::ranges`] counts on:
/// a lowercase form longer than one character keeps only its first.
fn fold(c: char) -> char {
    let c = match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        _ => c,
    };
    c.to_lowercase().next().unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use crate::{InputChar, PageFrame, Rect, TextModel};

    fn line(text: &str, y: f32) -> Vec<InputChar> {
        text.chars()
            .enumerate()
            .map(|(i, ch)| {
                let x = 50.0 + i as f32 * 10.0;
                InputChar {
                    ch,
                    bbox: [x, y, x + 10.0, y + 10.0],
                    size: 10.0,
                    angle: 0.0,
                    origin: Some((x, y + 1.2)),
                    source: 0,
                    space_before: false,
                }
            })
            .collect()
    }

    #[test]
    fn finds_across_lines_and_widths() {
        let mut input = line("選択精度の変化とＮｏ", 700.0);
        input.extend(line("数の関係", 685.0));
        let model = TextModel::build(&input, PageFrame::upright(Rect::new(0.0, 0.0, 595.0, 842.0)));
        let hits = model.find("no数");
        assert_eq!(hits.len(), 1);
        assert_eq!(model.text_of(&hits[0].chars), "Ｎｏ数");
        assert_eq!(model.find("精度").len(), 1);
        assert!(model.find(" ").is_empty());
    }

    #[test]
    fn maps_matches_past_inserted_spaces_to_the_original_characters() {
        let mut input = line("図Ａb𠮷Ａb", 700.0);
        input[2].space_before = true;
        input[5].space_before = true;
        let model = TextModel::build(&input, PageFrame::upright(Rect::new(0.0, 0.0, 595.0, 842.0)));
        let indices = |query| model.find(query).into_iter().map(|m| m.chars).collect::<Vec<_>>();
        assert_eq!(indices("a　B"), [vec![1, 2], vec![4, 5]]);
        assert_eq!(indices(" b"), [vec![2], vec![5]]);
        assert_eq!(indices("a "), [vec![1], vec![4]]);
        assert_eq!(indices("𠮷"), [vec![3]]);
    }
}
