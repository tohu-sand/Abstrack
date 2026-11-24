mod common;

use common::{BODY, LEFT, Page, RIGHT, column, document};
use textmodel::{Caret, Role, TextModel, span};

/// A two-column page with a running header, a one-column title and a page number.
fn page() -> Page {
    numbered(1)
}

/// Page `n`: the title is on the first page only, and the text differs from
/// page to page.
fn numbered(n: usize) -> Page {
    let mut p = Page::new();
    p.put(LEFT, 810.0, 8.0, "第1回講演会");
    if n == 1 {
        p.put(150.0, 770.0, 14.0, "全幅の表題が段の溝をまたぐ");
    }
    let (left, right) = if n == 1 { ('左', '右') } else { ('東', '西') };
    column(&mut p, LEFT, 700.0, left, 6);
    column(&mut p, RIGHT, 700.0, right, 6);
    p.put(295.0, 30.0, BODY, &n.to_string());
    p
}

/// Page `n` of a two-page document made of [`numbered`] pages.
fn in_document(n: usize) -> TextModel {
    document(&[numbered(1), numbered(2)]).swap_remove(n - 1)
}

fn text(m: &TextModel, a: usize, b: usize) -> String {
    m.text(a.min(b)..a.max(b))
}

#[test]
fn caret_snaps_to_the_nearest_character() {
    let m = page().model();
    // Halfway between the first two lines of the left column, over the 3rd character.
    let between = m.caret_at(LEFT + 2.2 * BODY, 700.0 - 7.5 + 4.0).unwrap();
    let line0 = m.caret_at(LEFT + 2.2 * BODY, 700.0 + 3.0).unwrap();
    let line1 = m.caret_at(LEFT + 2.2 * BODY, 685.0 + 3.0).unwrap();
    assert!(between == line0 || between == line1);
    // In the gap between the columns, the nearest character is at the end of a left line.
    let gutter = m.caret_at(LEFT + 24.0 * BODY + 4.0, 700.0 + 3.0).unwrap();
    assert_eq!(m.chars[gutter - 1].ch, '0');
    // Right half of a character puts the caret after it.
    let after = m.caret_at(LEFT + 0.8 * BODY, 700.0 + 3.0).unwrap();
    assert_eq!(text(&m, line0 - 2, after), "左");
}

#[test]
fn drag_from_left_column_into_right_column_follows_reading_order() {
    let m = page().model();
    let a = m.caret_at(LEFT + 20.2 * BODY, 640.0 + 3.0).unwrap(); // left column, 5th line
    let b = m.caret_at(RIGHT + 1.8 * BODY, 700.0 + 3.0).unwrap(); // right column, 1st line
    let selected = text(&m, a, b);
    assert!(selected.starts_with("左左左4"), "{selected}");
    assert!(selected.contains("左左左左左左左左左左左左左左左左左左左左左左左5"));
    assert!(selected.ends_with("右右"), "{selected}");
    assert!(!selected.contains('表'));
}

#[test]
fn rectangles_are_one_per_line() {
    let m = page().model();
    let a = m.caret_at(LEFT + 20.2 * BODY, 700.0 + 3.0).unwrap();
    let b = m.caret_at(LEFT + 3.2 * BODY, 670.0 + 3.0).unwrap();
    let rects = m.range_rects(a.min(b), a.max(b));
    assert_eq!(rects.len(), 3);
    // Line gaps stay unpainted: consecutive rectangles do not overlap vertically.
    assert!(rects[0][1] > rects[1][3] && rects[1][1] > rects[2][3]);
}

#[test]
fn words_follow_script_changes() {
    let mut p = Page::new();
    p.put(LEFT, 700.0, BODY, "試験プログラム数はNo180とした");
    let m = p.model();
    let word = |i: usize| {
        let (s, e) = m.word_at(i);
        m.text(s..e)
    };
    assert_eq!(word(0), "試験");
    assert_eq!(word(3), "プログラム");
    assert_eq!(word(9), "No");
    assert_eq!(word(11), "180");
    assert_eq!(m.word_boundary(0, true), 2);
    assert_eq!(m.word_boundary(2, true), 7);
    assert_eq!(m.word_boundary(9, false), 8);
}

#[test]
fn sentences_end_at_full_stops() {
    let mut p = Page::new();
    p.put(LEFT, 700.0, BODY, "第一の文である．第二の文");
    p.put(LEFT, 685.0, BODY, "は続く．第三");
    let m = p.model();
    let (s, e) = m.sentence_at(10);
    assert_eq!(m.text(s..e), "第二の文は続く．");
}

#[test]
fn page_body_leaves_out_header_and_page_number() {
    let m = in_document(1);
    let body = m.page_body();
    assert_eq!(m.text(0..body.start), "第1回講演会");
    assert_eq!(m.text(body.end..body.len), "1");
    // Without the other pages, only the page number is known for what it is.
    let alone = page().model();
    let body = alone.page_body();
    assert_eq!(body.start, 0);
    assert_eq!(alone.text(body.end..body.len), "1");
}

#[test]
fn selection_across_pages_skips_footer_and_header() {
    let m = in_document(1);
    let body = m.page_body();
    let models = [m.clone(), m];
    let ranges = span(
        Caret { page: 0, index: body.end - 5 },
        Caret { page: 1, index: body.start + 3 },
        |p| models[p].page_body(),
    );
    assert_eq!(ranges, vec![(0, body.end - 5, body.end), (1, body.start, body.start + 3)]);
    // Ending inside the running header of the next page takes in its start.
    let ranges = span(Caret { page: 0, index: body.end - 5 }, Caret { page: 1, index: 2 }, |p| models[p].page_body());
    assert_eq!(ranges[1], (1, 0, 2));
    // A caret right at the end of the body takes nothing of the page number,
    // so that extending a selection from there goes on into the next page.
    let ranges = span(Caret { page: 0, index: body.end }, Caret { page: 1, index: body.start + 1 }, |p| {
        models[p].page_body()
    });
    assert_eq!(ranges, vec![(1, body.start, body.start + 1)]);
}

#[test]
fn line_steps_keep_the_horizontal_position() {
    let m = in_document(1);
    let start = m.caret_at(LEFT + 5.2 * BODY, 700.0 + 3.0).unwrap();
    let down = m.line_step(start, true, true).unwrap();
    assert_eq!(m.chars[down].line, m.chars[start].line + 1);
    assert!((m.chars[down].display.x0 - m.chars[start].display.x0).abs() < 1.0);
    assert_eq!(m.line_step(down, false, true), Some(start));
    let first = m.page_body().start;
    assert!(m.line_step(0, false, false).is_none());
    assert!(m.line_step(first, false, false).is_some());
    // The end of a selection made forwards that stops at the end of a line
    // moves to the end of the next line, not of the one after.
    let line = m.chars[start].line;
    let end = m.lines[line].chars.end;
    assert_eq!(m.line_step(end, true, true), Some(m.lines[line + 1].chars.end));
    assert_eq!(m.line_step(end, true, false), Some(m.lines[line + 2].chars.start));
    // From the foot of the left column to the head of the right one, the
    // caret keeps its place within the column.
    let foot = m.caret_at(LEFT + 5.2 * BODY, 625.0 + 3.0).unwrap();
    let head = m.line_step(foot, true, true).unwrap();
    assert_eq!(m.text(m.lines[m.chars[head].line].chars.start..head), "右右右右右");
}

/// A running head that sits over one column only, as on the second page of
/// the fixture, or over the right one: it is still read before the columns,
/// so that a selection from the left column into the right one, or from the
/// previous page, does not take it in.
#[test]
fn running_head_over_one_column_is_read_first() {
    let page = |n: usize, head_x: f32| {
        let mut p = Page::new();
        p.put(head_x, 810.0, 8.0, "第99回講演論文集");
        let (left, right) = if n == 2 { ('左', '右') } else { ('東', '西') };
        column(&mut p, LEFT, 760.0, left, 6);
        column(&mut p, RIGHT, 760.0, right, 6);
        p.put(295.0, 30.0, BODY, &n.to_string());
        p
    };
    for head_x in [LEFT, RIGHT + 150.0] {
        let models = document(&[page(1, head_x), page(2, head_x), page(3, head_x)]);
        let m = &models[1];
        assert_eq!(m.blocks[0].role, Role::Header, "head at x={head_x}");
        assert_eq!(m.text(m.lines[0].chars.clone()), "第99回講演論文集");
        let a = m.caret_at(LEFT + 20.2 * BODY, 700.0 + 3.0).unwrap();
        let b = m.caret_at(RIGHT + 1.8 * BODY, 760.0 + 3.0).unwrap();
        assert!(!text(m, a, b).contains('講'), "head at x={head_x}");
        let body = m.page_body();
        assert!(m.text(body.start..body.end).starts_with('左'));
    }
}

/// Labels of a figure that happen to sit at the same height on two pages of
/// six are not a running head, and a caption at the foot of a page is not a
/// footer.
#[test]
fn repeated_labels_and_captions_stay_in_the_body() {
    let labels = ['甲', '乙', '丙', '丁', '戊', '己'];
    let pages: Vec<Page> = (1..=6)
        .map(|n| {
            let mut p = Page::new();
            if n == 3 || n == 4 {
                p.put(150.0, 790.0, BODY, "(a) Original (b) Revised");
            }
            column(&mut p, LEFT, 760.0, labels[n - 1], 6);
            column(&mut p, RIGHT, 760.0, labels[6 - n], 6);
            if n == 5 {
                p.put(150.0, 60.0, BODY, "Fig. 3 Outline of the pages");
            }
            p
        })
        .collect();
    for m in document(&pages) {
        assert!(m.blocks.iter().all(|b| b.role == Role::Body));
    }
}

/// A page whose only text is its number, such as a full-page figure, has an
/// empty body: a selection running through it takes nothing from it.
#[test]
fn page_with_only_its_number_has_an_empty_body() {
    let mut p = Page::new();
    p.put(295.0, 30.0, BODY, "7");
    let m = p.model();
    let body = m.page_body();
    assert_eq!(body.start, body.end);
    let models = [in_document(1), m, in_document(2)];
    let first = models[0].page_body();
    let ranges = span(
        Caret { page: 0, index: first.end - 2 },
        Caret { page: 2, index: first.start + 2 },
        |p| models[p].page_body(),
    );
    assert_eq!(ranges.len(), 2);
    assert!(ranges.iter().all(|r| r.0 != 1));
}

#[test]
fn triple_click_stops_at_a_heading_and_a_paragraph_break() {
    let mut p = Page::new();
    p.put(LEFT, 700.0, BODY, "1. 緒言");
    p.put(LEFT, 685.0, BODY, "　本要旨の記載内容はすべてが架空であり，文書は");
    p.put(LEFT, 670.0, BODY, "動作確認用として作成した．本要旨では段組みの効");
    p.put(LEFT, 655.0, BODY, "果を調べる．");
    p.put(LEFT, 640.0, BODY, "　次の段落である．");
    let m = p.model();
    let sentence = |line: usize, offset: usize| {
        let (s, e) = m.sentence_at(m.lines[line].chars.start + offset);
        m.text(s..e).replace('\n', "")
    };
    assert_eq!(sentence(0, 1), "1. 緒言");
    assert_eq!(sentence(1, 3), "本要旨の記載内容はすべてが架空であり，文書は動作確認用として作成した．");
    assert_eq!(sentence(3, 1), "本要旨では段組みの効果を調べる．");
    assert_eq!(sentence(4, 2), "次の段落である．");
}
