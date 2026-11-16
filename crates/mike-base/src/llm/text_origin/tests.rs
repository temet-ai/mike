use std::collections::BTreeMap;

use serde_json::json;

use super::compose::{Origin, compose};
use super::questions::{OPTIONS, ORIGIN, questions};
use super::signals::Signals;
use super::state::{Unmatched, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn input(text: &str) -> Unmatched {
    Unmatched {
        request_id: 7,
        session_db_id: 3,
        text: text.to_owned(),
        last_user_message: "the earlier message".to_owned(),
        previous_user_message: "the earlier message".to_owned(),
        previous_assistant_reply: "I fixed it and pushed.".to_owned(),
        author: "operator".to_owned(),
        has_agent_id: false,
        signals: Signals::default(),
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
fn state_keeps_short_text_whole() {
    let value = state(&input("continue"));
    assert_eq!(value["text"], json!("continue"));
}

#[test]
fn state_sends_the_text_what_it_follows_and_the_code_facts() {
    let value = state(&input("continue"));
    let mut keys: Vec<&str> = value
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    keys.sort_unstable();
    assert_eq!(keys, ["facts", "follows", "follows_is", "text"]);
}

#[test]
fn state_follows_the_reply_when_the_previous_message_is_wholly_machine_text() {
    let value = state(&with_previous(
        "continue",
        "<system-reminder>\nbuild output\n</system-reminder>",
        "all green",
    ));
    assert_eq!(value["follows"], json!("all green"));
    assert_eq!(value["follows_is"], json!("previous_assistant_reply"));
}

#[test]
fn state_follows_only_the_operators_words_of_a_previous_message_with_a_reminder() {
    let value = state(&with_previous(
        "continue",
        "<system-reminder>\nnoise\n</system-reminder>\nbuild the thing",
        "done",
    ));
    assert_eq!(value["follows"], json!("build the thing"));
    assert_eq!(value["follows_is"], json!("previous_user_message"));
}

#[test]
fn operator_comes_after_mail_in_the_options() {
    let keys: Vec<&str> = OPTIONS.iter().map(|(key, _)| *key).collect();
    let position = |name: &str| keys.iter().position(|key| *key == name);
    assert!(position("mail") < position("operator"));
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

fn with_previous(text: &str, previous_user: &str, reply: &str) -> Unmatched {
    Unmatched {
        previous_user_message: previous_user.to_owned(),
        previous_assistant_reply: reply.to_owned(),
        ..input(text)
    }
}

#[test]
fn state_follows_the_previous_user_message_when_it_differs_from_the_text() {
    let value = state(&with_previous("continue", "build the thing", "done"));
    assert_eq!(value["follows"], json!("build the thing"));
    assert_eq!(value["follows_is"], json!("previous_user_message"));
}

#[test]
fn state_follows_the_assistant_reply_when_the_previous_message_repeats_the_text() {
    let reply = format!("{}END", "r".repeat(500));
    let value = state(&with_previous("continue", "continue", &reply));
    let follows = value["follows"].as_str().unwrap_or_default();
    assert_eq!(follows.chars().count(), 300);
    assert!(follows.ends_with("END"));
    assert_eq!(value["follows_is"], json!("previous_assistant_reply"));
}

#[test]
fn state_follows_the_assistant_reply_when_there_is_no_previous_message() {
    let value = state(&with_previous("continue", "", "all green"));
    assert_eq!(value["follows"], json!("all green"));
    assert_eq!(value["follows_is"], json!("previous_assistant_reply"));
}

#[test]
fn state_follows_the_reply_when_the_previous_message_equals_the_last_user_message() {
    let mut repeated = with_previous("hello again", "the earlier message", "ok");
    repeated.last_user_message = "the earlier message".to_owned();
    let value = state(&repeated);
    assert_eq!(value["follows"], json!("ok"));
}
