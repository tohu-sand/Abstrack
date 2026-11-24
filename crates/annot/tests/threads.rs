use annot::thread::split_edit_text;
use annot::{AnnotKey, Annotation, Kind, PdfDate, ReplyType, Subtype, build_threads, latest_state, off_page};

fn annot(num: u32, subtype: Subtype) -> Annotation {
    Annotation {
        key: AnnotKey::Object((num, 0)),
        page: 0,
        index_on_page: num as usize,
        subtype,
        intent: None,
        rect: [0.0; 4],
        quads: Vec::new(),
        color: Vec::new(),
        opacity: None,
        author: Some("A".into()),
        contents: None,
        rich_contents: None,
        subject: None,
        name: None,
        created: None,
        modified: None,
        in_reply_to: None,
        reply_type: None,
        state_model: None,
        state: None,
        popup: None,
        parent: None,
        icon: None,
    }
}

fn date(s: &str) -> Option<PdfDate> {
    PdfDate::parse(s)
}

fn reply_to(mut a: Annotation, target: u32, rt: ReplyType) -> Annotation {
    a.in_reply_to = Some(AnnotKey::Object((target, 0)));
    a.reply_type = Some(rt);
    a
}

#[test]
fn replacement_pair_is_one_row() {
    let strike = reply_to(annot(10, Subtype::StrikeOut), 12, ReplyType::Group);
    let mut caret = annot(12, Subtype::Caret);
    caret.intent = Some("Replace".into());
    caret.contents = Some("Ht_x".into());
    let popup = annot(13, Subtype::Popup);
    let annots = vec![strike, annot(11, Subtype::Popup), caret, popup];
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].kind, Kind::Replace);
    assert_eq!(threads[0].head, 2);
    assert_eq!(threads[0].members, vec![0]);
}

#[test]
fn lone_caret_and_strikeout() {
    let annots = vec![annot(1, Subtype::Caret), annot(2, Subtype::StrikeOut)];
    let kinds: Vec<Kind> = build_threads(&annots).iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![Kind::Insert, Kind::Delete]);
}

#[test]
fn replies_are_flattened_and_sorted_oldest_first() {
    let head = annot(1, Subtype::Highlight);
    let mut late = reply_to(annot(2, Subtype::Text), 1, ReplyType::Reply);
    late.created = date("D:20260102000000+09'00'");
    let mut early = reply_to(annot(3, Subtype::Text), 1, ReplyType::Reply);
    early.created = date("D:20260101000000+09'00'");
    let mut nested = reply_to(annot(4, Subtype::Text), 2, ReplyType::Reply);
    nested.created = date("D:20260103000000+09'00'");
    let annots = vec![head, late, early, nested];
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].replies, vec![2, 1, 3]);
}

#[test]
fn states_attach_to_the_thread_and_latest_wins() {
    let head = annot(1, Subtype::Highlight);
    let mut on = reply_to(annot(2, Subtype::Text), 1, ReplyType::Reply);
    on.state_model = Some("Marked".into());
    on.state = Some("Marked".into());
    on.modified = date("D:20260101000000+09'00'");
    let mut off = reply_to(annot(3, Subtype::Text), 1, ReplyType::Reply);
    off.state_model = Some("Marked".into());
    off.state = Some("Unmarked".into());
    off.modified = date("D:20260101000100+09'00'");
    let mut review = reply_to(annot(4, Subtype::Text), 1, ReplyType::Reply);
    review.state_model = Some("Review".into());
    review.state = Some("Accepted".into());
    let annots = vec![head, on, off, review];
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 1);
    assert!(threads[0].replies.is_empty());
    assert_eq!(threads[0].states.len(), 3);
    let marked = latest_state(&annots, &threads[0], "Marked").unwrap();
    assert_eq!(marked.state.as_deref(), Some("Unmarked"));
    let review = latest_state(&annots, &threads[0], "Review").unwrap();
    assert_eq!(review.state.as_deref(), Some("Accepted"));
}

#[test]
fn state_on_a_grouped_member_reaches_the_head() {
    let strike = reply_to(annot(1, Subtype::StrikeOut), 2, ReplyType::Group);
    let caret = annot(2, Subtype::Caret);
    let mut state = reply_to(annot(3, Subtype::Text), 1, ReplyType::Reply);
    state.state_model = Some("Marked".into());
    state.state = Some("Marked".into());
    let annots = vec![strike, caret, state];
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].head, 1);
    assert_eq!(threads[0].states, vec![2]);
}

#[test]
fn review_states_chain_back_to_the_annotation() {
    let head = annot(1, Subtype::Highlight);
    let mut rejected = reply_to(annot(2, Subtype::Text), 1, ReplyType::Reply);
    rejected.state_model = Some("Review".into());
    rejected.state = Some("Rejected".into());
    rejected.created = date("D:20261121234640+09'00'");
    let mut none = reply_to(annot(3, Subtype::Text), 2, ReplyType::Reply);
    none.state_model = Some("Review".into());
    none.state = Some("None".into());
    none.created = date("D:20261121234650+09'00'");
    let annots = vec![head, rejected, none];
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].states, vec![1, 2]);
    let review = latest_state(&annots, &threads[0], "Review").unwrap();
    assert_eq!(review.state.as_deref(), Some("None"));
}

#[test]
fn state_without_target_is_not_a_row() {
    let mut orphan = annot(1, Subtype::Text);
    orphan.state_model = Some("Marked".into());
    orphan.state = Some("Unmarked".into());
    let mut dangling = reply_to(annot(2, Subtype::Text), 99, ReplyType::Reply);
    dangling.state_model = Some("Marked".into());
    dangling.state = Some("Marked".into());
    assert!(build_threads(&[orphan, dangling]).is_empty());
}

#[test]
fn reply_to_missing_annotation_is_shown_on_its_own() {
    let orphan = reply_to(annot(5, Subtype::Text), 99, ReplyType::Reply);
    let threads = build_threads(&[orphan]);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].kind, Kind::Note);
}

#[test]
fn replies_and_states_are_kept_off_the_page() {
    let head = annot(1, Subtype::Highlight);
    let reply = reply_to(annot(2, Subtype::Text), 1, ReplyType::Reply);
    let mut state = reply_to(annot(3, Subtype::Text), 1, ReplyType::Reply);
    state.state_model = Some("Marked".into());
    state.state = Some("Marked".into());
    let strike = reply_to(annot(4, Subtype::StrikeOut), 5, ReplyType::Group);
    let caret = annot(5, Subtype::Caret);
    let orphan = reply_to(annot(6, Subtype::Text), 99, ReplyType::Reply);
    let mut lost_state = annot(7, Subtype::Text);
    lost_state.state = Some("Unmarked".into());
    let note = annot(8, Subtype::Text);
    let boxed = reply_to(annot(9, Subtype::Square), 1, ReplyType::Reply);
    let annots = vec![head, reply, state, strike, caret, orphan, lost_state, note, boxed];
    let threads = build_threads(&annots);
    assert_eq!(off_page(&annots, &threads), vec![1, 2, 6]);
}

#[test]
fn irt_cycle_does_not_hang() {
    let a = reply_to(annot(1, Subtype::Text), 2, ReplyType::Reply);
    let b = reply_to(annot(2, Subtype::Text), 1, ReplyType::Reply);
    let threads = build_threads(&[a, b]);
    let placed: usize = threads.iter().map(|t| 1 + t.replies.len()).sum();
    assert_eq!(placed, 2);
}

#[test]
fn widgets_links_and_popups_are_not_rows() {
    let annots = vec![
        annot(1, Subtype::Link),
        annot(2, Subtype::Widget),
        annot(3, Subtype::Popup),
        annot(4, Subtype::Ink),
    ];
    let threads = build_threads(&annots);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].kind, Kind::Ink);
}

#[test]
fn edit_text_first_line_and_instruction() {
    assert_eq!(split_edit_text("Ht_x"), ("Ht_x", None));
    assert_eq!(split_edit_text("Ht_x\r下付きに"), ("Ht_x", Some("下付きに")));
    assert_eq!(split_edit_text("語\r\n\r\n説明\r"), ("語", Some("説明")));
    assert_eq!(split_edit_text("語\r"), ("語", None));
}
