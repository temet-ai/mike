use std::collections::BTreeMap;

use super::compose::compose;
use super::injected::is_injected;
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
fn compose_at_or_above_the_correction_edge_is_a_correction() {
    assert_eq!(compose(&answered(0.7)).map(|c| c.kind), Some("correction"));
    assert_eq!(compose(&answered(0.95)).map(|c| c.kind), Some("correction"));
}

#[test]
fn compose_at_or_below_the_clarification_edge_is_a_clarification() {
    assert_eq!(
        compose(&answered(0.3)).map(|c| c.kind),
        Some("clarification")
    );
    assert_eq!(
        compose(&answered(0.05)).map(|c| c.kind),
        Some("clarification")
    );
}

#[test]
fn compose_between_the_edges_records_no_kind_so_the_keyword_kind_stands() {
    for p in [0.31, 0.5, 0.55, 0.62, 0.69] {
        assert!(compose(&answered(p)).is_none(), "{p}");
    }
}

#[test]
fn mail_peer_and_channel_envelopes_are_injected_text() {
    assert!(is_injected("Mail from worker-2 (session abc): done"));
    assert!(is_injected(
        "  <channel source=\"mike\" kind=\"x\">hi</channel>"
    ));
    assert!(is_injected(
        "Another Claude session sent a message:\nstop that"
    ));
}

#[test]
fn a_message_wholly_of_system_reminder_blocks_is_injected_text() {
    assert!(is_injected(
        "<system-reminder>\nno, that is wrong\n</system-reminder>"
    ));
    assert!(is_injected(
        "<system-reminder id=\"a1\">x</system-reminder>\n<system-reminder>y</system-reminder>"
    ));
}

#[test]
fn an_operator_message_is_not_injected_text() {
    assert!(!is_injected("no, that is wrong"));
    assert!(!is_injected("The mail from Bob was wrong, fix it"));
    assert!(!is_injected(
        "<system-reminder>x</system-reminder>\nno, that is wrong"
    ));
    assert!(!is_injected("<system-reminder>unclosed"));
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
