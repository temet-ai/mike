use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{ONLY_NUDGE, questions};
use super::state;
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::guard::label_rules::is_prod;

fn answered(p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        ONLY_NUDGE.to_owned(),
        Answer::Noul {
            noul: p,
            confidence: None,
        },
    )]))
}

#[test]
fn questions_ask_one_noul() {
    assert_eq!(questions().len(), 1);
}

#[test]
fn state_carries_the_message_and_its_length() {
    let value = state("  no, use the other one  ");
    assert_eq!(value["message"], "no, use the other one");
    assert_eq!(value["message_chars"], 21);
}

#[test]
fn compose_short_message_with_content_is_not_a_prod_even_though_the_regex_says_so() {
    assert!(is_prod("no, use the other"));
    let Some(verdict) = compose(&answered(0.1)) else {
        panic!("an answered question must compose");
    };
    assert!(!verdict.is_prod);
    assert!((verdict.score() - 0.9).abs() < 1e-9);
}

#[test]
fn compose_long_nudge_is_a_prod_even_though_the_regex_says_it_is_not() {
    let message = "please keep going with whatever you were doing before";
    assert!(!is_prod(message));
    let Some(verdict) = compose(&answered(0.95)) else {
        panic!("an answered question must compose");
    };
    assert!(verdict.is_prod);
}

#[test]
fn compose_failed_call_gives_the_off_path_result() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}
