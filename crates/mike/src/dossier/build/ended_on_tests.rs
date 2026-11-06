use anyhow::{Context, Result};
use serde_json::Value;

use super::ended_on::{
    EndedOn, INPUT_CHARS, TextSpan, classify, find_bounded, last_line, non_sidecar_text,
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/dossier/ended_on_d_py.json");

fn code(text: &str) -> &'static str {
    classify(text).class.code()
}

fn span(origin: &str, start: usize, end: usize, request: i64) -> TextSpan {
    TextSpan {
        origin: origin.to_string(),
        start,
        end,
        first_request_id: request,
        last_request_id: request,
    }
}

#[test]
fn d_py_fixture_table_matches() -> Result<()> {
    let rows: Vec<Value> = serde_json::from_str(FIXTURE)?;
    assert!(rows.len() >= 40, "fixture rows: {}", rows.len());
    for row in rows {
        let name = row["name"].as_str().context("name")?;
        let text = row["text"].as_str().context("text")?;
        let expected = row["expected"].as_str().context("expected")?;
        assert_eq!(code(text), expected, "case {name}");
    }
    Ok(())
}

#[test]
fn empty_text_is_none() {
    assert_eq!(code(""), "none");
    assert_eq!(classify("").span, None);
}

#[test]
fn wait_phrase_401_characters_from_end_is_not_promise() {
    let text = format!("I'll report back {}closing", "x".repeat(385));
    assert_eq!(code(&text), "other");
    let inside = format!("I'll report back {}", "x".repeat(370));
    assert_eq!(code(&inside), "promise_or_wait");
}

#[test]
fn handback_phrase_501_characters_from_end_is_not_handback() {
    let outside = format!("want me to {}", "x".repeat(495));
    assert_eq!(code(&outside), "other");
    let inside = format!("want me to {}", "x".repeat(480));
    assert_eq!(code(&inside), "handback");
}

#[test]
fn handback_wins_over_question_and_question_over_promise() {
    assert_eq!(code("Shall I go ahead?"), "handback");
    assert_eq!(code("I'll report back soon. Anything else?"), "question");
    assert_eq!(
        code("All done. I'll report back tomorrow."),
        "promise_or_wait"
    );
}

#[test]
fn done_as_substring_of_word_is_not_report() {
    assert_eq!(code("The abandoned branch was removed."), "other");
    assert_eq!(code("the flag is_done_now stays"), "other");
    assert_eq!(code("The refactor is complete."), "report");
}

#[test]
fn last_line_splits_on_unicode_line_separator() {
    assert_eq!(
        last_line("First line\u{2028}Is this the end?"),
        "Is this the end?"
    );
    assert_eq!(last_line("  \n\t"), "");
    assert_eq!(last_line("a\r\nb\r\n"), "b");
}

#[test]
fn find_bounded_respects_word_edges() {
    assert_eq!(find_bounded("abandoned done", "done"), Some(10));
    assert_eq!(find_bounded("undone", "done"), None);
    assert_eq!(find_bounded("done_x", "done"), None);
    assert_eq!(find_bounded("x", ""), None);
}

#[test]
fn span_covers_deciding_match_in_characters() {
    let text = "Zażółć. Shall I proceed";
    let class = classify(text);
    assert_eq!(class.class, EndedOn::Handback);
    let matched = class.span.map(|(start, end)| {
        text.chars()
            .skip(start)
            .take(end - start)
            .collect::<String>()
    });
    assert_eq!(
        matched.map(|m| m.to_lowercase()),
        Some("shall i".to_string())
    );
}

#[test]
fn span_maps_through_text_spans_to_final_assistant_text_offsets() {
    let final_text = "Ala ma kota. SIDECAR BLOCK HERE.Let me check the build";
    let sidecar_len = "SIDECAR BLOCK HERE.".chars().count();
    let spans = vec![
        span("primary", 0, 13, 1),
        span("monitor_sidecar", 13, 13 + sidecar_len, 2),
        span("primary", 13 + sidecar_len, final_text.chars().count(), 3),
    ];
    let input = non_sidecar_text(final_text, &spans);
    assert_eq!(input.text, "Ala ma kota. Let me check the build");
    let class = input.classify();
    assert_eq!(class.class, EndedOn::PromiseOrWait);
    let matched = class.span.map(|(start, end)| {
        final_text
            .chars()
            .skip(start)
            .take(end - start)
            .collect::<String>()
    });
    assert_eq!(
        matched.map(|m| m.to_lowercase()),
        Some("let me".to_string())
    );
}

#[test]
fn sidecar_only_text_is_none() {
    let final_text = "Status block";
    let spans = vec![span("monitor_sidecar", 0, 12, 1)];
    assert_eq!(
        non_sidecar_text(final_text, &spans).classify().class,
        EndedOn::None
    );
}

#[test]
fn input_keeps_only_the_last_4000_characters() {
    let body = format!("{} {}", "a".repeat(INPUT_CHARS + 50), "Shall I go?");
    let len = body.chars().count();
    let input = non_sidecar_text(&body, &[span("primary", 0, len, 1)]);
    assert_eq!(input.text.chars().count(), INPUT_CHARS);
    assert_eq!(input.classify().class, EndedOn::Handback);
}
