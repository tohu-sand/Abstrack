//! `--threads`: annotations grouped the way the comment list shows them.

use annot::thread::{split_edit_text, state_model_of};
use annot::{Annotation, Kind, Subtype, Thread, build_threads, latest_state, read_all};
use pdf_io::PdfFile;
use std::collections::BTreeMap;

pub fn print_threads(file: &PdfFile) {
    let annots = read_all(file);
    let threads = build_threads(&annots);

    let popups = annots.iter().filter(|a| a.subtype == Subtype::Popup).count();
    let non_markup = annots
        .iter()
        .filter(|a| !a.subtype.is_markup() && a.subtype != Subtype::Popup)
        .count();
    let mut placed = vec![false; annots.len()];
    for t in &threads {
        for &i in std::iter::once(&t.head)
            .chain(&t.members)
            .chain(&t.replies)
            .chain(&t.states)
        {
            placed[i] = true;
        }
    }
    let in_threads = placed.iter().filter(|&&p| p).count();
    let markup = annots.iter().filter(|a| a.subtype.is_markup()).count();
    let unplaced: Vec<&Annotation> = annots
        .iter()
        .zip(&placed)
        .filter(|(a, p)| a.subtype.is_markup() && !**p)
        .map(|(a, _)| a)
        .collect();
    let orphan_states = unplaced.iter().filter(|a| a.is_state()).count();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &threads {
        *kinds.entry(t.kind.label()).or_default() += 1;
    }
    println!(
        "comments: {}  ({})",
        threads.len(),
        kinds
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "annotations: {}  markup: {}  placed in threads: {}  popup: {}  other: {}",
        annots.len(),
        markup,
        in_threads,
        popups,
        non_markup
    );
    if orphan_states > 0 {
        println!("states without a target (not listed): {orphan_states}");
    }
    for a in unplaced.iter().filter(|a| !a.is_state()) {
        println!("WARNING: {} {} is not in any thread", a.subtype.name(), a.key);
    }

    for (n, t) in threads.iter().enumerate() {
        print_thread(&annots, n + 1, t);
    }
}

fn print_thread(annots: &[Annotation], number: usize, t: &Thread) {
    let head = &annots[t.head];
    let mut parts = vec![format!("{} {}", head.subtype.name(), head.key)];
    parts.extend(
        t.members
            .iter()
            .map(|&m| format!("{} {}", annots[m].subtype.name(), annots[m].key)),
    );
    println!();
    println!(
        "#{number} p{} {}  [{}]  {}  {}",
        head.page + 1,
        t.kind.label(),
        parts.join(" + "),
        head.author.as_deref().unwrap_or("(作成者なし)"),
        head.created
            .or(head.modified)
            .map(|d| d.to_string())
            .unwrap_or_else(|| "(日時なし)".into())
    );
    let carries_edit = matches!(t.kind, Kind::Replace | Kind::Insert);
    for &i in std::iter::once(&t.head).chain(t.members.iter()) {
        let a = &annots[i];
        let Some(text) = a.contents.as_deref() else {
            continue;
        };
        if carries_edit && a.subtype == Subtype::Caret {
            let (edit, note) = split_edit_text(text);
            let label = if t.kind == Kind::Replace { "置換後" } else { "挿入" };
            println!("    {label}: {}", quoted(Some(edit)));
            if let Some(note) = note {
                println!("    指示: {}", quoted(Some(note)));
            }
        } else if i == t.head {
            println!("    本文: {}", quoted(Some(text)));
        } else {
            println!("    本文({}): {}", a.subtype.name(), quoted(Some(text)));
        }
    }
    for &r in &t.replies {
        let a = &annots[r];
        println!(
            "    返信 {} {} {}: {}",
            a.key,
            a.author.as_deref().unwrap_or("(作成者なし)"),
            a.created.or(a.modified).map(|d| d.to_string()).unwrap_or_default(),
            quoted(a.contents.as_deref())
        );
    }
    for &s in &t.states {
        let a = &annots[s];
        println!(
            "    状態 {} {}={} {} {}",
            a.key,
            state_model_of(a),
            a.state.as_deref().unwrap_or("?"),
            a.author.as_deref().unwrap_or("(作成者なし)"),
            a.date().map(|d| d.to_string()).unwrap_or_default()
        );
    }
    if let Some(m) = latest_state(annots, t, "Marked") {
        println!("    チェック: {}", m.state.as_deref() == Some("Marked"));
    }
}

fn quoted(text: Option<&str>) -> String {
    match text {
        None => "(なし)".into(),
        Some(t) => format!("\"{}\"", t.replace('\r', "\\r").replace('\n', "\\n")),
    }
}
