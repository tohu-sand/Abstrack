mod common;

use common::{BODY, LEFT, Page, RIGHT, column, line_texts};

#[test]
fn title_then_two_columns_then_page_number() {
    let mut p = Page::new();
    p.put(LEFT, 810.0, 8.0, "ヘッダー");
    p.put(150.0, 770.0, 14.0, "全幅の表題が段の溝をまたぐ");
    column(&mut p, LEFT, 700.0, '左', 6);
    column(&mut p, RIGHT, 700.0, '右', 6);
    p.put(295.0, 30.0, BODY, "1");
    let m = p.model();
    let lines = line_texts(&m);
    assert_eq!(lines[0], "ヘッダー");
    assert_eq!(lines[1], "全幅の表題が段の溝をまたぐ");
    assert!(lines[2..8].iter().all(|l| l.starts_with('左')), "{lines:?}");
    assert!(lines[8..14].iter().all(|l| l.starts_with('右')), "{lines:?}");
    assert_eq!(lines[14], "1");
    assert_eq!(lines.len(), 15);
}

#[test]
fn source_order_does_not_matter() {
    let mut p = Page::new();
    column(&mut p, RIGHT, 700.0, '右', 6);
    p.put(295.0, 30.0, BODY, "1");
    column(&mut p, LEFT, 700.0, '左', 6);
    p.put(150.0, 770.0, 14.0, "全幅の表題が段の溝をまたぐ");
    let m = p.model();
    let lines = line_texts(&m);
    assert_eq!(lines[0], "全幅の表題が段の溝をまたぐ");
    assert!(lines[1].starts_with('左') && lines[7].starts_with('右'), "{lines:?}");
    assert_eq!(lines.last().unwrap(), "1");
}

#[test]
fn subscripts_and_superscripts_stay_on_their_line() {
    let mut p = Page::new();
    let x = p.put(LEFT, 700.0, BODY, "数は Nt");
    let x = p.put_raised(x, 700.0, 7.0, -2.5, "α");
    let x = p.put(x, 700.0, BODY, " = 180 と");
    let x = p.put_raised(x, 700.0, 7.0, 3.8, "+");
    p.put(x, 700.0, BODY, "で");
    p.put(LEFT, 685.0, BODY, "次の行");
    let m = p.model();
    let lines = line_texts(&m);
    assert_eq!(lines, vec!["数は Ntα = 180 と+で", "次の行"]);
}

#[test]
fn full_width_caption_between_column_bands() {
    let mut p = Page::new();
    column(&mut p, LEFT, 780.0, 'A', 5);
    column(&mut p, RIGHT, 780.0, 'B', 5);
    // Centred under the two columns (x 50..170 and 310..430), reaching well into both.
    p.put(135.0, 600.0, 10.0, "図2 全幅の図の説明文で段の溝をまたいでいる");
    column(&mut p, LEFT, 560.0, 'C', 2);
    column(&mut p, RIGHT, 560.0, 'D', 2);
    let m = p.model();
    let firsts: String = line_texts(&m).iter().map(|l| l.chars().next().unwrap()).collect();
    assert_eq!(firsts, "AAAAABBBBB図CCDD");
}

#[test]
fn single_column_with_ragged_lines_is_one_block() {
    let mut p = Page::new();
    for k in 0..12 {
        let len = 10 + (k * 7) % 30;
        let text: String = std::iter::repeat_n('字', len).collect();
        p.put(LEFT, 780.0 - 15.0 * k as f32, BODY, &text);
    }
    let m = p.model();
    assert_eq!(m.blocks.len(), 1);
    assert_eq!(m.lines.len(), 12);
}

#[test]
fn table_cells_are_read_row_by_row() {
    let mut p = Page::new();
    for row in 0..6 {
        let y = 700.0 - 15.0 * row as f32;
        p.put(LEFT, y, BODY, &format!("r{row}a"));
        p.put(200.0, y, BODY, &format!("r{row}b"));
        p.put(350.0, y, BODY, &format!("r{row}c"));
    }
    let m = p.model();
    let all = m.text(0..m.chars.len());
    let expected: Vec<String> = (0..6).flat_map(|r| ["a", "b", "c"].map(|c| format!("r{r}{c}"))).collect();
    assert_eq!(all, expected.join(" "));
}

#[test]
fn quads_select_reading_order_characters() {
    let mut p = Page::new();
    p.put(LEFT, 700.0, BODY, "abcdefghij");
    let m = p.model();
    // Covers "cde": x from 60 to 75.
    let q = [(60.0, 709.0), (75.0, 709.0), (60.0, 698.0), (75.0, 698.0)];
    let hits = m.chars_in_quads(&[q]);
    assert_eq!(m.text_of(&hits), "cde");
    assert_eq!(m.insertion_point(70.0, 699.0), Some(4));
}

/// A fraction in a display equation: the denominator is read right after the
/// numerator, before the rest of the line and the equation number, so that a
/// drag over the fraction takes in nothing else. Two rows of a table set the
/// same way, with nothing at half height beside them, stay apart.
#[test]
fn fractions_are_read_numerator_then_denominator() {
    let mut p = Page::new();
    let x = p.put(LEFT, 700.0, BODY, "s = c");
    p.put_raised(x + 1.0, 700.0, BODY, 5.0, "dN");
    let after = p.put_raised(x + 1.0, 700.0, BODY, -8.0, "dt");
    p.put(after + 1.0, 700.0, BODY, "|t=0 ,");
    p.put(280.0, 700.0, BODY, "(3)");
    let m = p.model();
    let lines = line_texts(&m);
    assert!(lines.iter().any(|l| l.contains("cdNdt|t=0")), "{lines:#?}");
    let all = m.text(0..m.chars.len());
    let start = all.chars().position(|c| c == 'd').unwrap();
    assert_eq!(m.text(start..start + 4), "dNdt");

    let mut t = Page::new();
    t.put(LEFT, 700.0, BODY, "No");
    t.put(100.0, 700.0, BODY, "1.10");
    t.put(100.0, 689.0, BODY, "1.06");
    let m = t.model();
    assert_eq!(line_texts(&m), vec!["No 1.10", "1.06"]);
}


/// A first page: a title and a long abstract set full width above two
/// columns. The full-width lines do not hide the columns below them.
#[test]
fn full_width_abstract_above_two_columns() {
    let mut p = Page::new();
    p.put(150.0, 800.0, 14.0, "表題");
    for k in 0..10 {
        p.put(LEFT, 770.0 - 12.0 * k as f32, BODY, &"全幅の要旨の行".repeat(7));
    }
    column(&mut p, LEFT, 600.0, 'A', 12);
    column(&mut p, RIGHT, 600.0, 'B', 12);
    let m = p.model();
    let firsts: String = line_texts(&m).iter().map(|l| l.chars().next().unwrap()).collect();
    assert_eq!(firsts, format!("表{}{}{}", "全".repeat(10), "A".repeat(12), "B".repeat(12)));
}

/// Lines of one column broken by a formula at about the same place are not
/// two columns: what follows the formula starts anywhere, not at a column
/// edge.
#[test]
fn lines_broken_by_formulas_are_not_columns() {
    let mut p = Page::new();
    for k in 0..12 {
        let y = 760.0 - 15.0 * k as f32;
        if (3..7).contains(&k) {
            p.put(LEFT, y, BODY, &"a".repeat(28));
            p.put_raised(195.0, y, 7.0, 3.0, "x");
            p.put(205.0 + 7.0 * k as f32, y, BODY, &"b".repeat(40));
        } else {
            p.put(LEFT, y, BODY, &"c".repeat(90));
        }
    }
    let m = p.model();
    assert_eq!(m.blocks.len(), 1);
}

/// A small label of a figure in the right column that runs a little into
/// the gap between the columns belongs to that column; it does not cut the
/// columns into bands read across.
#[test]
fn a_label_straying_into_the_gap_stays_in_its_column() {
    let mut p = Page::new();
    column(&mut p, LEFT, 700.0, 'A', 6);
    column(&mut p, RIGHT, 700.0, 'B', 6);
    p.put(RIGHT - 10.0, 662.0, 7.0, "Inset");
    let m = p.model();
    let firsts: String = line_texts(&m).iter().map(|l| l.chars().next().unwrap()).collect();
    assert_eq!(firsts, "AAAAAABBBIBBB");
}
