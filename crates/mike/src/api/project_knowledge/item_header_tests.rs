use serde_json::{Value, json};

use super::*;

fn row(kind: &str) -> Value {
    json!({
        "source_kind": kind,
        "title": "t",
        "path": null,
        "updated_at": "2026-11-08T14:03:22+00:00",
        "author": {"role": "session", "session_title": "Search ranking work"},
        "provenance": {}
    })
}

#[test]
fn header_for_a_note_with_a_note_kind_names_kind_date_and_session() {
    let mut note = row("note");
    note["provenance"] = json!({"kind": "decision", "tags": []});

    assert_eq!(
        header(&note),
        "note, decision | 2026-11-08 | Search ranking work"
    );
}

#[test]
fn header_for_a_lesson_with_a_topic_names_kind_and_topic_without_a_name() {
    let mut lesson = row("lesson");
    lesson["author"] = Value::Null;
    lesson["provenance"] = json!({"lesson_kind": "correction", "topic": "database-migrations"});

    assert_eq!(
        header(&lesson),
        "lesson, correction, database-migrations | 2026-11-08"
    );
}

#[test]
fn header_for_a_lesson_without_a_topic_leaves_the_topic_out() {
    let mut lesson = row("lesson");
    lesson["provenance"] = json!({"lesson_kind": "correction", "topic": null});

    assert_eq!(
        header(&lesson),
        "lesson, correction | 2026-11-08 | Search ranking work"
    );
}

#[test]
fn header_for_a_row_with_no_name_has_two_parts() {
    let mut mail = row("mail");
    mail["author"] = json!({"role": "session"});

    assert_eq!(header(&mail), "mail | 2026-11-08");
}

#[test]
fn header_for_a_row_with_no_date_leaves_the_date_out() {
    let mut turn = row("turn");
    turn["updated_at"] = Value::Null;

    assert_eq!(header(&turn), "turn | Search ranking work");
}

#[test]
fn header_for_a_document_names_its_path() {
    let mut document = row("artifact");
    document["path"] = json!("docs/SEARCH.md");

    assert_eq!(header(&document), "artifact | 2026-11-08 | docs/SEARCH.md");
}

#[test]
fn header_for_a_work_thread_item_names_the_thread_slug() {
    let mut entry = row("thread_entry");
    entry["provenance"] = json!({"slug": "search-speed", "entry_kind": "state"});

    assert_eq!(header(&entry), "thread_entry | 2026-11-08 | search-speed");
}

#[test]
fn header_for_a_session_overview_names_the_session_title() {
    let overview = row("session");

    assert_eq!(
        header(&overview),
        "session | 2026-11-08 | Search ranking work"
    );
}

#[test]
fn header_with_a_long_multi_line_name_is_one_cut_line() {
    let mut note = row("note");
    note["author"] = json!({"session_title": format!("first\nsecond {}", "x".repeat(200))});

    let text = header(&note);

    assert!(!text.contains('\n'));
    assert!(text.ends_with("..."));
    assert!(text.chars().count() < 120);
}

#[test]
fn turn_header_with_a_session_title_names_date_and_title() {
    assert_eq!(
        turn_header("2026-11-08", Some("Search work")),
        "turn | 2026-11-08 | Search work"
    );
}

#[test]
fn turn_header_without_a_session_title_has_two_parts() {
    assert_eq!(turn_header("2026-11-08", None), "turn | 2026-11-08");
    assert_eq!(turn_header("2026-11-08", Some("  ")), "turn | 2026-11-08");
}

#[test]
fn with_header_puts_the_header_first_and_cuts_to_the_document_cap() {
    let text = with_header("note | 2026-11-08", &"b".repeat(5_000));

    assert!(text.starts_with("note | 2026-11-08\nb"));
    assert_eq!(text.chars().count(), MAX_DOCUMENT_CHARS);
}
