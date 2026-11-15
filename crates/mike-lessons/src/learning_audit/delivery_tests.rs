use serde_json::{Value, json};

use super::delivery::render;
use super::promotion::PromotionState;
use super::render::render as render_report;
use super::report::Report;

fn arms() -> Value {
    json!({"arms": [
        {"arm": "mod", "sessions": 3, "operator_turns": 10, "lessons_delivered": 25,
         "lessons_per_turn": 2.5, "requests": 400, "usage_requests": 390, "input_tokens": 100,
         "cache_read_tokens": 800, "cache_write_tokens": 100, "cache_read_share": 0.8,
         "judged_turns": 8, "corrected_turns": 2, "corrected_rate": 0.25},
        {"arm": "control", "sessions": 1, "operator_turns": 0, "lessons_delivered": 0,
         "lessons_per_turn": null, "requests": 50, "usage_requests": 0, "input_tokens": 0,
         "cache_read_tokens": 0, "cache_write_tokens": 0, "cache_read_share": null,
         "judged_turns": 0, "corrected_turns": 0, "corrected_rate": null},
    ]})
}

#[test]
fn delivery_shows_every_rate_with_the_counts_it_comes_from() {
    let text = render(&arms()).join("\n");
    assert!(text.contains(
        "| mod | 3 | 10 | 2.50 (25 lessons / 10 turns) | 80.0% (800 / 1000 input tokens) | 25.0% (2 corrected / 8 judged) |"
    ));
    assert!(text.contains(
        "| control | 1 | 0 | no data (0 lessons / 0 turns) | no data (0 / 0 input tokens) | no data (0 corrected / 0 judged) |"
    ));
    assert!(text.contains("- mod: 390 of 400 requests carried token usage."));
}

#[test]
fn delivery_says_so_when_neither_arm_made_a_request() {
    let mut empty = arms();
    empty["arms"][0]["sessions"] = json!(0);
    empty["arms"][1]["sessions"] = json!(0);
    assert!(
        render(&empty)
            .join("\n")
            .contains("No session in either arm made a request")
    );
}

#[test]
fn report_carries_the_delivery_section_only_when_the_audit_fetched_it() {
    let mut report = Report::default();
    let state = PromotionState::default();
    assert!(!render_report(&report, &state).contains("Lesson delivery"));
    report.lesson_delivery = Some(arms());
    assert!(
        render_report(&report, &state)
            .contains("## Lesson delivery: mike mod against proxy injection")
    );
}
