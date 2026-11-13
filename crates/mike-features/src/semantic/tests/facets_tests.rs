use serde_json::json;

use super::facets::{ending_text, prompt_head};

fn span(origin: &str, start: usize, end: usize) -> serde_json::Value {
    json!({"origin": origin, "start": start, "end": end, "first_request_id": 1, "last_request_id": 1})
}

#[test]
fn ending_text_removes_every_sidecar_span_not_only_the_trailing_one() {
    let text = "Done. SIDE-A The tests pass. SIDE-B";
    let spans = json!([
        span("primary", 0, 6),
        span("steer_sidecar", 6, 13),
        span("primary", 13, 28),
        span("recap_sidecar", 28, 35),
    ]);
    let ending = ending_text(text, &spans, 1_024);
    assert_eq!(ending, "Done. The tests pass.");
    assert!(!ending.contains("SIDE"));
}

#[test]
fn ending_text_cuts_back_to_whitespace_within_budget() {
    let text = "alpha bravo charlie delta echo";
    let ending = ending_text(text, &json!([]), 12);
    assert_eq!(ending, "delta echo");
}

#[test]
fn ending_text_empty_when_only_sidecar_text() {
    let spans = json!([span("recap_sidecar", 0, 9)]);
    assert_eq!(ending_text("sidecar!!", &spans, 1_024), "");
}

#[test]
fn prompt_facet_is_chunk_zero_as_chunker_cuts_it() {
    let long = format!("{}\n\n{}", "a".repeat(20), "b".repeat(20));
    let head = prompt_head(&long, 25);
    assert_eq!(head.as_deref(), Some("a".repeat(20).as_str()));
    assert_eq!(prompt_head("   ", 25), None);
}
