use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{DONE_SINCE, STILL_OPEN};
use super::state::{WakeInput, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn answered(open: f64, done: f64) -> Asked {
    let noul = |p| Answer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        (STILL_OPEN.to_owned(), noul(open)),
        (DONE_SINCE.to_owned(), noul(done)),
    ]))
}

#[test]
fn compose_open_concern_with_nothing_done_is_delivered() {
    let r = compose(&answered(0.9, 0.1));
    assert_eq!(r.map(|r| r.skip), Some(false));
}

#[test]
fn compose_concern_answered_since_is_skipped() {
    let r = compose(&answered(0.1, 0.9));
    assert_eq!(r.map(|r| r.skip), Some(true));
}

#[test]
fn compose_failed_ask_is_none_so_the_wake_is_pushed_as_today() {
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn json_scores_a_skip_by_the_chance_the_nudge_was_not_needed() {
    let r = compose(&answered(0.1, 0.9));
    let score = r.map(|r| r.json(false)["score"].as_f64().unwrap_or(-1.0));
    assert!(score.is_some_and(|s| (s - 0.9).abs() < 1e-9));
}

#[test]
fn state_without_facts_still_carries_the_nudge_and_question() {
    let input = WakeInput {
        nudge: "You said you would wait for the build.",
        question: Some("unbacked_wait"),
        facts: None,
    };
    let s = state(&input);
    assert_eq!(s["question"], "unbacked_wait");
    assert!(s["facts"].is_null());
    assert_eq!(s["nudge"], "You said you would wait for the build.");
}
