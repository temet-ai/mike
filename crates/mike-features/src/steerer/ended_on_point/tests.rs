use std::collections::BTreeMap;

use super::compose::{Ending, apply, compose};
use super::questions::{CLAIMS_DONE, ENDINGS, ENDS_ON, questions};
use super::state;
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::guard::gates::{EndedOn, phrase_gates};

fn answers(ends: &str, ends_p: f64, done_p: f64) -> Asked {
    Asked::answered(BTreeMap::from([
        (
            ENDS_ON.to_owned(),
            Answer::Choice {
                choice: ends.to_owned(),
                probabilities: BTreeMap::from([(ends.to_owned(), ends_p)]),
                confidence: None,
            },
        ),
        (
            CLAIMS_DONE.to_owned(),
            Answer::Noul {
                noul: done_p,
                confidence: None,
            },
        ),
    ]))
}

#[test]
fn questions_ask_the_ending_choice_and_the_done_noul() {
    let asked = questions();
    assert_eq!(asked.len(), 2);
    assert!(asked.contains_key(ENDS_ON));
    assert!(asked.contains_key(CLAIMS_DONE));
    for (key, _) in ENDINGS {
        assert!(Ending::parse(key).is_some());
    }
}

#[test]
fn state_keeps_the_end_of_a_long_turn() {
    let long = format!("{}THE END", "x".repeat(5_000));
    let value = state(&long);
    let text = value["turn_end"].as_str().unwrap_or_default();
    assert!(text.ends_with("THE END"));
    assert!(text.chars().count() <= 1_500);
}

#[test]
fn compose_reads_both_answers() {
    let Some(reading) = compose(&answers("handback", 0.9, 0.8)) else {
        panic!("answered questions must compose");
    };
    assert_eq!(reading.ending, Ending::Handback);
    assert!(reading.claims_done);
    assert!(reading.score() > 0.8);
}

#[test]
fn compose_failed_call_gives_the_off_path_result() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn apply_replaces_the_regex_reading_where_the_two_disagree() {
    let regex_phrase = phrase_gates("All done. I'll check back later.", "");
    assert!(regex_phrase.g2);
    let Some(reading) = compose(&answers("report", 0.9, 0.9)) else {
        panic!("answered questions must compose");
    };
    let phrase = apply(regex_phrase, &reading);
    assert_eq!(phrase.ended_on, EndedOn::Report);
    assert!(!phrase.g2);
    assert!(!phrase.g3);
    assert!(phrase.g4);
    assert!(phrase.done_match.is_some());
}

#[test]
fn apply_promise_opens_the_wait_gate_without_a_regex_match() {
    let regex_phrase = phrase_gates("Tests pass.", "");
    assert!(!regex_phrase.g2);
    let Some(reading) = compose(&answers("promise", 0.8, 0.1)) else {
        panic!("answered questions must compose");
    };
    let phrase = apply(regex_phrase, &reading);
    assert!(phrase.g2);
    assert_eq!(phrase.ended_on, EndedOn::PromiseOrWait);
    assert_eq!(phrase.wait_match.as_deref(), Some("jev"));
    assert!(!phrase.g4);
}
