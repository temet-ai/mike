use std::collections::HashMap;

use serde_json::{Value, json};

use super::*;

fn item(reference: &str, kind: &str, title: &str) -> Value {
    json!({
        "reference": reference,
        "source_kind": kind,
        "title": title,
        "status": null,
        "path": "docs/search.md",
        "updated_at": "2026-11-09T14:03:22+00:00",
        "author": {"role": "operator"},
        "copies": 1,
        "also": [],
        "also_count": 0,
        "excerpt": "e".repeat(300),
        "rank": 0.03,
        "lexical_rank": 0.12,
        "semantic_rank": 0.81,
        "identifier_hits": 0,
        "provenance": {"tier": "operator"}
    })
}

fn answer(items: Vec<Value>) -> Value {
    json!({
        "project_uuid": "5b0f6f0e-5d51-4c9a-8e43-0c8f7a6d2b11",
        "retrieval_path": "hybrid",
        "items": items,
        "has_more": true,
        "next_offset": 10,
        "read_tool": "read_project_knowledge",
        "embedding": {
            "coverage": {"total": 4121, "ready": 4087, "pending": 34, "failed": 0},
            "model": "embeddinggemma:300m",
            "fallback": null,
            "scope": "artifact_lesson_note",
            "turns": "existing model-matched turn embeddings"
        },
        "rerank": {"state": "reranked", "ready_in_s": 0, "left_out": 0, "cached": 50}
    })
}

fn ten_results() -> Vec<Value> {
    (0..10)
        .map(|n| {
            item(
                &format!("turn:{}", 40_000 + n),
                "turn",
                "Why search ranks mail on its own merits",
            )
        })
        .collect()
}

fn passages(reference: &str, text: &str) -> Passages {
    HashMap::from([(reference.to_owned(), text.to_owned())])
}

fn rich_item() -> Value {
    let mut row = item("turn:40000", "turn", "Why search ranks mail");
    row["author"] = json!({"role": "operator", "session_uuid": SESSION});
    row["provenance"] = json!({"tier": "operator", "written_by": "machine", "outside_project": true, "ranked_lower": true});
    row["identifier_hits"] = json!(2);
    row["also"] = json!(["turn:1", "turn:2", "turn:3", "turn:4", "turn:5"]);
    row["also_count"] = json!(5);
    row["rerank_score"] = json!(1.5);
    row
}

fn names(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

const SESSION: &str = "36106608-f114-4e6e-91ef-182745aa7165";

#[test]
fn best_only_first_item_carries_exactly_the_listed_fields() {
    let mut shaped = answer(vec![rich_item(), item("turn:2", "turn", "second")]);

    best_only(&mut shaped, &passages("turn:40000", &"p".repeat(2_000)));

    let first = &shaped["items"][0];
    assert_eq!(
        names(first),
        vec![
            "also",
            "also_count",
            "excerpt",
            "identifier_hits",
            "provenance",
            "reference",
            "session_uuid",
            "source_kind",
            "title",
            "updated_at"
        ]
    );
    assert_eq!(first["also"], json!(["turn:1", "turn:2", "turn:3"]));
    assert_eq!(first["also_count"], 5);
    assert_eq!(first["identifier_hits"], 2);
    assert_eq!(first["session_uuid"], SESSION);
    assert_eq!(
        first["provenance"],
        json!({"written_by": "machine", "outside_project": true})
    );
}

#[test]
fn best_only_first_item_gets_the_passage_up_to_1500_characters_and_the_marker() {
    let mut first = item("turn:40000", "turn", "Why");
    first["identifier_hits"] = json!(0);
    let mut shaped = answer(vec![first]);

    best_only(&mut shaped, &passages("turn:40000", &"p".repeat(2_000)));

    let first = &shaped["items"][0];
    assert_eq!(
        first["excerpt"].as_str().map(|e| e.chars().count()),
        Some(1_500)
    );
    assert_eq!(first["excerpt_from"], "passage");
    assert_eq!(
        names(first),
        vec![
            "excerpt",
            "excerpt_from",
            "reference",
            "source_kind",
            "title",
            "updated_at"
        ]
    );
}

#[test]
fn best_only_first_item_with_a_700_character_title_is_cut_to_200() {
    let long = "word ".repeat(140);
    let mut shaped = answer(vec![item("turn:40000", "turn", &long)]);

    best_only(&mut shaped, &Passages::new());

    let title = shaped["items"][0]["title"].as_str().unwrap_or_default();
    assert_eq!(title.chars().count(), 200);
    assert!(title.starts_with("word word"));
}

#[test]
fn best_only_first_item_takes_the_session_from_provenance_when_author_has_none() {
    let mut row = item("turn:1", "turn", "Why");
    row["provenance"] = json!({"session_uuid": SESSION});
    let mut shaped = answer(vec![row]);

    best_only(&mut shaped, &Passages::new());

    assert_eq!(shaped["items"][0]["session_uuid"], SESSION);
    assert!(shaped["items"][0].get("provenance").is_none());
}

#[test]
fn best_only_envelope_holds_only_the_listed_fields() {
    let mut shaped = answer(ten_results());

    best_only(&mut shaped, &Passages::new());

    assert_eq!(
        names(&shaped),
        vec![
            "has_more",
            "items",
            "more",
            "next_offset",
            "project_uuid",
            "rerank"
        ]
    );
    assert_eq!(
        shaped["rerank"],
        json!({"state": "reranked", "left_out": 0})
    );
}

#[test]
fn best_only_envelope_keeps_the_embedding_fallback_and_nothing_else_of_embedding() {
    let mut shaped = answer(ten_results());
    shaped["embedding"]["fallback"] = json!("embedding_unavailable");

    best_only(&mut shaped, &Passages::new());

    assert_eq!(
        shaped["embedding"],
        json!({"fallback": "embedding_unavailable"})
    );
}

#[test]
fn best_only_envelope_keeps_rerank_detail_and_a_nonzero_wait_but_not_cached() {
    let mut shaped = answer(ten_results());
    shaped["rerank"] =
        json!({"state": "loading", "ready_in_s": 4, "detail": "loading", "cached": 3});

    best_only(&mut shaped, &Passages::new());

    assert_eq!(
        shaped["rerank"],
        json!({"state": "loading", "detail": "loading", "ready_in_s": 4})
    );
}

#[test]
fn best_only_reduces_every_later_item_to_exactly_a_reference_and_a_line() {
    let mut shaped = answer(ten_results());

    best_only(&mut shaped, &Passages::new());

    let items = shaped["items"].as_array().map_or(&[][..], Vec::as_slice);
    assert_eq!(items.len(), 10);
    for (position, later) in items.iter().enumerate().skip(1) {
        let mut fields: Vec<&str> = later
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_default();
        fields.sort_unstable();
        assert_eq!(fields, vec!["line", "reference"], "item {position}");
        assert_eq!(later["reference"], format!("turn:{}", 40_000 + position));
    }
}

#[test]
fn best_only_without_a_passage_leaves_the_first_excerpt_as_it_is() {
    let mut shaped = answer(ten_results());

    best_only(&mut shaped, &Passages::new());

    assert_eq!(shaped["items"][0]["excerpt"], "e".repeat(300));
    assert!(shaped["items"][0].get("excerpt_from").is_none());
}

#[test]
fn best_only_first_item_with_identifier_hits_keeps_its_identifier_excerpt() {
    let mut first = item("turn:1", "turn", "title");
    first["identifier_hits"] = json!(2);
    first["excerpt"] = json!("around ISSUE-42");
    let mut shaped = answer(vec![first, item("turn:2", "turn", "second")]);

    best_only(&mut shaped, &passages("turn:1", "a passage"));

    assert_eq!(shaped["items"][0]["excerpt"], "around ISSUE-42");
    assert!(shaped["items"][0].get("excerpt_from").is_none());
}

#[test]
fn best_only_says_how_to_get_the_rest() {
    let mut shaped = answer(ten_results());

    best_only(&mut shaped, &Passages::new());

    assert_eq!(
        shaped["more"],
        json!({"shown_in_full": 1, "pointers": 9, "how": HOW_TO_GET_THE_REST})
    );
    assert_eq!(shaped["has_more"], true);
    assert_eq!(shaped["next_offset"], 10);
}

#[test]
fn best_only_single_result_without_a_next_page_keeps_more_with_no_pointers() {
    let mut shaped = answer(vec![item("turn:1", "turn", "only")]);
    shaped["has_more"] = json!(false);

    best_only(&mut shaped, &Passages::new());

    assert_eq!(
        shaped["more"],
        json!({"shown_in_full": 1, "pointers": 0, "how": HOW_TO_GET_THE_REST})
    );
}

#[test]
fn best_only_how_asks_the_session_to_cite_the_reference() {
    assert!(HOW_TO_GET_THE_REST.contains("Cite the reference you use."));
}

#[test]
fn best_only_answer_without_items_is_left_alone() {
    let mut shaped = answer(Vec::new());
    let before = shaped.clone();

    best_only(&mut shaped, &Passages::new());

    assert_eq!(shaped["items"], before["items"]);
}

#[test]
fn line_is_kind_date_title_and_marks_machine_written_rows() {
    let mut row = item("mail:1", "mail", "Stop hook feedback");
    row["provenance"] = json!({"written_by": "machine"});

    assert_eq!(
        line(&row),
        "mail \u{b7} 2026-11-09 \u{b7} Stop hook feedback \u{b7} machine-written"
    );
    assert_eq!(
        line(&item("note:2", "note", "Plain note")),
        "note \u{b7} 2026-11-09 \u{b7} Plain note"
    );
}

#[test]
fn line_of_a_note_starts_with_the_note_kind_after_the_item_kind() {
    let mut row = item("note:3", "note", "Use the shared pool");
    row["provenance"] = json!({"kind": "decision"});

    assert_eq!(
        line(&row),
        "note, decision \u{b7} 2026-11-09 \u{b7} Use the shared pool"
    );
}

#[test]
fn line_of_a_lesson_with_a_topic_cuts_the_title_to_keep_120_characters() {
    let mut row = item("lesson:4", "lesson", &"word ".repeat(60));
    row["provenance"] = json!({"lesson_kind": "correction", "topic": "database-migrations"});

    let shaped = line(&row);

    assert!(
        shaped.starts_with("lesson, correction, database-migrations \u{b7} 2026-11-09 \u{b7} word")
    );
    assert!(shaped.ends_with("..."));
    assert_eq!(shaped.chars().count(), LINE_CHARS);
}

#[test]
fn line_of_a_session_overview_starts_with_the_session_kind() {
    let row = item("session:5", "session", "Reranker header work");

    assert_eq!(
        line(&row),
        "session \u{b7} 2026-11-09 \u{b7} Reranker header work"
    );
}

#[test]
fn line_of_an_outside_project_row_ends_with_the_suffix_after_machine_written() {
    let mut row = item("mail:1", "mail", "Stop hook feedback");
    row["provenance"] = json!({"written_by": "machine", "outside_project": true});

    assert_eq!(
        line(&row),
        "mail \u{b7} 2026-11-09 \u{b7} Stop hook feedback \u{b7} machine-written \u{b7} outside this project"
    );
}

#[test]
fn line_of_an_outside_project_row_with_a_long_title_still_fits_the_limit() {
    let mut row = item("mail:1", "mail", &"w".repeat(300));
    row["provenance"] = json!({"written_by": "machine", "outside_project": true});

    let shaped = line(&row);

    assert_eq!(shaped.chars().count(), LINE_CHARS);
    assert!(shaped.ends_with("... \u{b7} machine-written \u{b7} outside this project"));
}

#[test]
fn line_long_multibyte_title_is_cut_to_the_line_limit_in_characters() {
    let mut row = item("mail:1", "mail", &"\u{e9}\u{4e16}".repeat(200));
    row["provenance"] = json!({"written_by": "machine"});

    let shaped = line(&row);

    assert_eq!(shaped.chars().count(), LINE_CHARS);
    assert!(shaped.ends_with("... \u{b7} machine-written"), "{shaped}");
}

#[test]
fn line_title_with_newlines_is_one_line_and_missing_date_is_undated() {
    let mut row = item("note:1", "note", "first\n\nsecond\tthird");
    row["updated_at"] = Value::Null;

    assert_eq!(line(&row), "note \u{b7} undated \u{b7} first second third");
}

#[test]
fn best_only_worst_case_ten_results_serialise_under_4000_characters() {
    let long_title = "w".repeat(120);
    let reference = |n: usize| format!("lesson:5b0f6f0e-5d51-4c9a-8e43-0c8f7a6d2b{n:02}");
    let mut results: Vec<Value> = (0..10)
        .map(|n| item(&reference(n), "lesson", &long_title))
        .collect();
    results[0]["also"] = json!([reference(11), reference(12), reference(13)]);
    results[0]["also_count"] = json!(3);
    results[0]["author"] = json!({"role": "operator", "session_uuid": SESSION});
    let mut shaped = answer(results);

    best_only(&mut shaped, &passages(&reference(0), &"p".repeat(2_000)));

    let size = shaped.to_string().chars().count();
    assert!(size < 4_000, "{size} characters");
}
