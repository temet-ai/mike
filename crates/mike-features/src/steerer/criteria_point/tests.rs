use std::collections::BTreeMap;

use super::compose::{compose, mean_confidence};
use super::questions::{key, questions};
use super::state;
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::criteria_contract::{CriterionVerdict, classify};

fn answered(ps: &[f64]) -> Asked {
    Asked::answered(
        ps.iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    key(i),
                    Answer::Noul {
                        noul: *p,
                        confidence: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn questions_ask_one_noul_per_criterion() {
    assert_eq!(questions(3).len(), 3);
}

#[test]
fn state_keys_each_criterion_by_position() {
    let value = state(&["The page loads", "  Add a panel  "]);
    assert_eq!(value["criteria"][key(0)], "The page loads");
    assert_eq!(value["criteria"][key(1)], "Add a panel");
}

#[test]
fn compose_accepts_at_or_above_one_half() {
    assert_eq!(
        compose(&answered(&[0.9, 0.2, 0.5]), 3),
        Some(vec![true, false, true])
    );
}

#[test]
fn compose_failed_call_gives_the_off_path_result() {
    assert_eq!(compose(&Asked::failed_with("timeout"), 2), None);
}

#[test]
fn compose_missing_answer_gives_the_off_path_result() {
    assert_eq!(compose(&answered(&[0.9]), 2), None);
}

#[test]
fn the_verb_list_rejects_a_state_sentence_that_opens_with_a_listed_verb() {
    let sentence = "Use of the shared button is visible on every page.";
    assert!(matches!(classify(sentence), CriterionVerdict::Rejected(_)));
    let Some(accepted) = compose(&answered(&[0.9]), 1) else {
        panic!("an answered question must compose");
    };
    assert_eq!(accepted, vec![true]);
}

#[test]
fn mean_confidence_is_the_average_of_the_stronger_side() {
    let asked = answered(&[0.9, 0.2]);
    assert!((mean_confidence(&asked, 2) - 0.85).abs() < 1e-9);
}
