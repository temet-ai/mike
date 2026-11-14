use std::collections::BTreeMap;

use serde_json::json;

use super::compose::{Origin, compose};
use super::questions::{OPTIONS, ORIGIN, questions};
use super::state::{Unmatched, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn input(text: &str) -> Unmatched {
    Unmatched {
        request_id: 7,
        session_db_id: 3,
        text: text.to_owned(),
        last_user_message: "the earlier message".to_owned(),
        author: "operator".to_owned(),
        has_agent_id: false,
    }
}

fn chose(key: &str, probability: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        ORIGIN.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), probability)]),
            confidence: None,
        },
    )]))
}

#[test]
fn questions_offer_every_origin_the_compose_step_parses() {
    assert_eq!(questions().len(), 1);
    for (key, _) in OPTIONS {
        assert!(Origin::parse(key).is_some(), "{key} must parse");
    }
}

#[test]
fn compose_operator_answer_applies_and_clears_the_repeat_key() {
    let verdict = compose(&chose("operator", 0.9));
    let Some(verdict) = verdict else {
        panic!("an answered question must compose");
    };
    assert_eq!(verdict.origin, Origin::Operator);
    assert!(verdict.applies());
    assert!(verdict.clears_repeat_key());
    assert_eq!(verdict.origin.kind_code(), "operator");
}

#[test]
fn compose_loop_answer_maps_to_the_loop_wakeup_kind_and_keeps_the_repeat_key() {
    let Some(verdict) = compose(&chose("loop_wakeup", 0.85)) else {
        panic!("an answered question must compose");
    };
    assert_eq!(verdict.origin.kind_code(), "loop-wakeup");
    assert!(!verdict.clears_repeat_key());
}

#[test]
fn compose_low_probability_answer_does_not_apply() {
    let Some(verdict) = compose(&chose("mail", 0.4)) else {
        panic!("an answered question must compose");
    };
    assert!(!verdict.applies());
}

#[test]
fn compose_failed_call_gives_the_off_path_result() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn compose_unknown_choice_gives_no_verdict() {
    assert_eq!(compose(&chose("martian", 0.99)), None);
}

#[test]
fn state_keeps_short_text_whole_and_reports_its_length() {
    let value = state(&input("continue"));
    assert_eq!(value["text"], json!("continue"));
    assert_eq!(value["text_chars"], json!(8));
    assert_eq!(value["author_guess"], json!("operator"));
}

#[test]
fn state_cuts_the_middle_of_long_text_and_keeps_both_ends() {
    let long = format!("{}{}{}", "a".repeat(1000), "MIDDLE", "z".repeat(1000));
    let value = state(&input(&long));
    let text = value["text"].as_str().unwrap_or_default();
    assert!(text.starts_with("aaa"));
    assert!(text.ends_with("zzz"));
    assert!(!text.contains("MIDDLE"));
    assert!(text.contains("[...]"));
}
