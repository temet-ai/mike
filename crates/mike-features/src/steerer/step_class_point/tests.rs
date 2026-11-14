use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{CLASS, CLASSES, questions};
use super::state;
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::guard::step_class::{StepClass, classify_sentence};

fn chose(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        CLASS.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

#[test]
fn every_offered_class_parses_to_a_step_class() {
    assert_eq!(CLASSES.len(), StepClass::ALL.len());
    for (key, _) in CLASSES {
        assert!(StepClass::from_code(key).is_some(), "{key}");
    }
    assert_eq!(questions().len(), 1);
}

#[test]
fn state_carries_the_sentence_and_the_end_of_the_turn() {
    let value = state(
        "Should I push back on the review?",
        "...Should I push back on the review?",
    );
    assert_eq!(value["sentence"], "Should I push back on the review?");
    assert!(
        value["turn_end"]
            .as_str()
            .unwrap_or_default()
            .contains("review")
    );
}

#[test]
fn compose_picks_the_class_jev_names_over_the_regex_match() {
    assert_eq!(
        classify_sentence("Should I push back on the review?"),
        StepClass::Push
    );
    assert_eq!(
        compose(&chose("proceed", 0.8)),
        Some((StepClass::Proceed, 0.8))
    );
}

#[test]
fn compose_low_probability_leaves_the_regex_class() {
    assert_eq!(compose(&chose("push", 0.3)), None);
}

#[test]
fn compose_failed_call_gives_the_off_path_result() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn compose_unknown_class_key_gives_no_choice() {
    assert_eq!(compose(&chose("teleport", 0.9)), None);
}
