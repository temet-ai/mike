use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::CORRECTS;
use super::state::state;
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn answered(p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        CORRECTS.to_owned(),
        Answer::Noul {
            noul: p,
            confidence: None,
        },
    )]))
}

#[test]
fn compose_at_the_threshold_is_a_correction() {
    assert_eq!(compose(&answered(0.75)).map(|c| c.kind), Some("correction"));
}

#[test]
fn compose_below_the_threshold_is_a_clarification() {
    assert_eq!(
        compose(&answered(0.74)).map(|c| c.kind),
        Some("clarification")
    );
}

#[test]
fn compose_failed_ask_is_none_so_the_keyword_kind_stands() {
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn json_scores_a_clarification_by_the_chance_it_is_not_a_correction() {
    let c = compose(&answered(0.1));
    let score = c.map(|c| c.json()["score"].as_f64().unwrap_or(-1.0));
    assert!((score.unwrap_or(-1.0) - 0.9).abs() < 1e-9);
}

#[test]
fn state_keeps_the_closing_claim_of_the_prior_turn() {
    let prior = format!("{}All tests pass.", "z".repeat(9_000));
    let s = state("no, that is wrong", &prior);
    let end = s["prior_turn_end"].as_str().unwrap_or_default();
    assert!(end.ends_with("All tests pass."));
    assert_eq!(end.chars().count(), 2_500);
}
