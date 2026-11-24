//! Selections are ranges of caret positions in reading order. A caret position
//! `i` lies before character `i`; `chars.len()` is after the last character.

use crate::{Block, Role, TextModel};

/// Character classes whose boundaries delimit words; Japanese text has no
/// spaces, so a change of script is a word boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Latin,
    Digit,
    Hiragana,
    Katakana,
    Kanji,
    Punct,
    Other,
}

fn class(c: char) -> Class {
    match c {
        _ if c.is_whitespace() => Class::Space,
        '0'..='9' | '０'..='９' => Class::Digit,
        'ー' => Class::Katakana,
        '\u{3041}'..='\u{309F}' => Class::Hiragana,
        '\u{30A0}'..='\u{30FF}' | '\u{FF66}'..='\u{FF9F}' => Class::Katakana,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '々' => Class::Kanji,
        _ if c.is_alphabetic() => Class::Latin,
        _ if c.is_ascii_punctuation() || ('\u{3000}'..='\u{303F}').contains(&c) || ('\u{FF01}'..='\u{FF0F}').contains(&c) => {
            Class::Punct
        }
        _ => Class::Other,
    }
}

/// Words that end in a full stop inside a sentence.
const ABBREVIATIONS: &[&str] = &[
    "Fig", "Figs", "Eq", "Eqs", "Ref", "Refs", "Sec", "Secs", "Tab", "Ch", "Chap", "Vol", "No", "pp", "al", "cf",
    "vs", "ca", "Dr", "Prof",
];

impl TextModel {
    /// The caret position nearest to a user-space point. The nearest character
    /// is found by the distance to the centres of the character boxes, so a
    /// point between lines or columns snaps to text; the caret goes before or
    /// after it depending on which half of the character the point is in.
    pub fn caret_at(&self, x: f32, y: f32) -> Option<usize> {
        let (dx, dy) = self.frame.to_display(x, y);
        let (best, _) = self
            .chars
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let (cx, cy) = c.display.center();
                (i, (cx - dx).powi(2) + (cy - dy).powi(2))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let c = &self.chars[best];
        Some(if dx > c.display.center().0 { best + 1 } else { best })
    }

    /// One rectangle per line for the characters in `start..end`, user space.
    pub fn range_rects(&self, start: usize, end: usize) -> Vec<[f32; 4]> {
        let chars = self.chars.get(start..end.min(self.chars.len())).unwrap_or(&[]);
        let Some(first) = chars.first() else { return Vec::new() };
        let mut out = Vec::with_capacity(chars.last().unwrap().line - first.line + 1);
        for line in chars.chunk_by(|a, b| a.line == b.line) {
            let r = line[1..].iter().fold(line[0].rect, |r, c| r.union(&c.rect));
            out.push([r.x0, r.y0, r.x1, r.y1]);
        }
        out
    }

    /// The word around caret position `at` (for a double click): a run of
    /// characters of the same class, never crossing a line.
    pub fn word_at(&self, at: usize) -> (usize, usize) {
        if self.chars.is_empty() {
            return (0, 0);
        }
        let i = at.min(self.chars.len() - 1);
        let line = self.chars[i].line;
        let k = class(self.chars[i].ch);
        let same = |j: usize| self.chars[j].line == line && class(self.chars[j].ch) == k;
        let mut start = i;
        while start > 0 && same(start - 1) {
            start -= 1;
        }
        let mut end = i + 1;
        while end < self.chars.len() && same(end) {
            end += 1;
        }
        (start, end)
    }

    /// The sentence around caret position `at` (for a triple click): from after
    /// the previous full stop to after the next one, within the block and the
    /// paragraph. A heading on a line of its own is a sentence by itself.
    pub fn sentence_at(&self, at: usize) -> (usize, usize) {
        if self.chars.is_empty() {
            return (0, 0);
        }
        let i = at.min(self.chars.len() - 1);
        let block = self.lines[self.chars[i].line].block;
        let in_block = |j: usize| self.lines[self.chars[j].line].block == block;
        // Caret position j is at the end of a line that closes a paragraph.
        let breaks = |j: usize| {
            let line = self.chars[j - 1].line;
            self.lines[line].chars.end == j && self.paragraph_ends(line)
        };
        let mut start = i;
        while start > 0 && in_block(start - 1) && !self.ends_sentence(start - 1) && !breaks(start) {
            start -= 1;
        }
        let mut end = i;
        while end < self.chars.len() && in_block(end) {
            end += 1;
            if self.ends_sentence(end - 1) || breaks(end) {
                break;
            }
        }
        while start < end && self.chars[start].ch.is_whitespace() {
            start += 1;
        }
        (start, end)
    }

    /// Whether character `j` ends a sentence. A full stop does not when it
    /// follows a section number at the start of a line ("1." "2.3．"), an
    /// abbreviation ("Fig." "et al."), or is followed at once by more of the
    /// word ("3.14" "e.g."); the Japanese "．" is followed at once by the
    /// next sentence and ends it.
    fn ends_sentence(&self, j: usize) -> bool {
        let c = self.chars[j].ch;
        if !matches!(c, '。' | '．' | '.' | '!' | '?' | '！' | '？') {
            return false;
        }
        if !matches!(c, '.' | '．') {
            return true;
        }
        let line = &self.lines[self.chars[j].line];
        let mut k = j;
        while k > line.chars.start && !self.chars[k].space_before && !self.chars[k - 1].ch.is_whitespace() {
            k -= 1;
        }
        let word: String = self.chars[k..j].iter().map(|c| c.ch).collect();
        let numbering = k == line.chars.start
            && !word.is_empty()
            && word.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || matches!(c, '.' | '．'));
        if numbering {
            return false;
        }
        if c == '.' {
            let joined = self
                .chars
                .get(j + 1)
                .is_some_and(|n| n.line == self.chars[j].line && !n.space_before && !n.ch.is_whitespace());
            let abbreviation = ABBREVIATIONS.contains(&word.rsplit('.').next().unwrap_or(""));
            return !joined && !abbreviation;
        }
        true
    }

    /// Whether a paragraph ends with line `l`: the line stops well short of
    /// the block's right edge, or the next line is indented.
    fn paragraph_ends(&self, l: usize) -> bool {
        let line = &self.lines[l];
        let Some(next) = self.lines.get(l + 1).filter(|n| n.block == line.block) else {
            return true;
        };
        let block = &self.blocks[line.block].display;
        let size = self.chars[line.chars.clone()].iter().map(|c| c.size).fold(0.0f32, f32::max);
        let first = &self.chars[next.chars.start];
        let short = line.display.x1 < block.x1 - 2.0 * size;
        let indented = first.ch == '\u{3000}' || first.display.x0 > block.x0 + 0.8 * first.size;
        short || indented
    }

    /// The caret position on the next (or previous) line in reading order at
    /// the same horizontal position, for Shift+Down and Shift+Up. `None` on
    /// the last (or first) line of the page.
    /// `after`: a caret between the last character of one line and the first
    /// of the next is taken to be at the end of the first (when it ends a
    /// selection made forwards) rather than at the start of the second.
    pub fn line_step(&self, at: usize, forward: bool, after: bool) -> Option<usize> {
        let (line, offset) = self.caret_column(at, after)?;
        let target = if forward { line + 1 } else { line.checked_sub(1)? };
        (target < self.lines.len()).then(|| self.caret_on_line(target, offset))
    }

    /// The line of caret position `at` and its distance from the left edge
    /// of the line's block, so that a caret moving from the foot of one
    /// column to the head of the next keeps its place within the column.
    /// `after` as for [`TextModel::line_step`].
    pub fn caret_column(&self, at: usize, after: bool) -> Option<(usize, f32)> {
        let n = self.chars.len();
        let (line, x) = if (after && at > 0) || at >= n {
            let c = self.chars.get(at.min(n).checked_sub(1)?)?;
            (c.line, c.display.x1)
        } else {
            let c = &self.chars[at];
            (c.line, c.display.x0)
        };
        Some((line, x - self.blocks[self.lines[line].block].display.x0))
    }

    /// The caret position on `line` at `offset` from the left edge of its block.
    pub fn caret_on_line(&self, line: usize, offset: f32) -> usize {
        let x = self.blocks[self.lines[line].block].display.x0 + offset;
        let range = self.lines[line].chars.clone();
        range
            .clone()
            .find(|&k| self.chars[k].display.center().0 > x)
            .unwrap_or(range.end)
    }

    /// The next word boundary after (or before, for `forward == false`) caret
    /// position `at`, for Ctrl+Shift+Arrow.
    pub fn word_boundary(&self, at: usize, forward: bool) -> usize {
        let n = self.chars.len();
        let key = |j: usize| (self.chars[j].line, class(self.chars[j].ch));
        if forward {
            if at >= n {
                return n;
            }
            let mut j = at;
            let k = key(j);
            while j < n && key(j) == k {
                j += 1;
            }
            while j < n && class(self.chars[j].ch) == Class::Space {
                j += 1;
            }
            j
        } else {
            if at == 0 {
                return 0;
            }
            let mut j = at;
            while j > 0 && class(self.chars[j - 1].ch) == Class::Space {
                j -= 1;
            }
            if j == 0 {
                return 0;
            }
            let k = key(j - 1);
            while j > 0 && key(j - 1) == k {
                j -= 1;
            }
            j
        }
    }

    /// Caret positions bounding the body of the page: running headers above
    /// and page numbers or footers below (see [`Role`]) are left out when a
    /// selection runs past the start or end of the page. A page with nothing
    /// but margin text has an empty body after its headers.
    pub fn body_range(&self) -> (usize, usize) {
        let chars = |b: &Block| (self.lines[b.lines.start].chars.start, self.lines[b.lines.end - 1].chars.end);
        let mut body = self.blocks.iter().filter(|b| b.role == Role::Body);
        match body.next() {
            Some(first) => (chars(first).0, chars(body.next_back().unwrap_or(first)).1),
            None => {
                let after_headers = self
                    .blocks
                    .iter()
                    .take_while(|b| b.role == Role::Header)
                    .last()
                    .map_or(0, |b| chars(b).1);
                (after_headers, after_headers)
            }
        }
    }
}

/// A caret position on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    pub page: usize,
    pub index: usize,
}

/// Where a page's body starts and ends, and how many characters the page has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageBody {
    pub start: usize,
    pub end: usize,
    pub len: usize,
}

impl TextModel {
    pub fn page_body(&self) -> PageBody {
        let (start, end) = self.body_range();
        PageBody {
            start,
            end,
            len: self.chars.len(),
        }
    }
}

/// The ranges of characters a selection between two carets covers, one
/// (page, start, end) per page it takes characters from, in reading order.
/// Where the selection runs on past the end of a page or comes in from the
/// previous one, the page's running header and page number are left out; a
/// caret placed inside them takes them in.
pub fn span(a: Caret, b: Caret, body: impl Fn(usize) -> PageBody) -> Vec<(usize, usize, usize)> {
    (a.page.min(b.page)..=a.page.max(b.page)).filter_map(|page| {
        let r = span_on_page(a, b, page, || body(page));
        (!r.is_empty()).then_some((page, r.start, r.end))
    }).collect()
}

/// The part of a selection on one page, so a long drag can use one model at a time.
pub fn span_on_page(a: Caret, b: Caret, page: usize, body: impl FnOnce() -> PageBody) -> std::ops::Range<usize> {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    if page < from.page || page > to.page {
        return 0..0;
    }
    if from.page == to.page {
        return from.index..to.index;
    }
    let body = body();
    if page == from.page {
        from.index..if from.index > body.end { body.len } else { body.end }
    } else if page == to.page {
        (if to.index < body.start { 0 } else { body.start })..to.index
    } else {
        body.start..body.end
    }
}

/// The text of the ranges [`span`] gives, in order, the pages' text joined
/// as [`joins_with_space`] says. `model` gives the model of a page.
pub fn span_text<'a>(ranges: &[(usize, usize, usize)], model: impl Fn(usize) -> &'a TextModel) -> String {
    let mut text = String::new();
    for &(page, start, end) in ranges {
        model(page).append_page_text(start..end, &mut text);
    }
    text
}

/// Whether text from two places (the end of one line or page and the start of
/// the next) is joined with a space: only between Latin letters or digits.
pub fn joins_with_space(prev: char, next: char) -> bool {
    let latin = |c: char| c.is_ascii_alphanumeric() || matches!(c, '\u{00C0}'..='\u{024F}' | '\u{0370}'..='\u{03FF}');
    latin(prev) && latin(next) && prev != '-'
}
