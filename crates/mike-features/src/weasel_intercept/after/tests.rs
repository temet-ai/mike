use std::collections::BTreeMap;

use super::compose::{compose, state};
use super::questions::{USED_AS_WEASEL, questions};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::weasel_intercept::scanner::DeltaScanner;

fn answered(p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        USED_AS_WEASEL.to_owned(),
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
fn state_carries_the_word_and_the_end_of_the_text() {
    let long = format!("{}it is robust", "x".repeat(2_000));
    let value = state("robust", &long);
    assert_eq!(value["word"], "robust");
    assert!(
        value["text"]
            .as_str()
            .unwrap_or_default()
            .ends_with("it is robust")
    );
    assert!(value["text"].as_str().unwrap_or_default().chars().count() <= 500);
}

#[test]
fn compose_labels_a_real_weasel_and_an_ordinary_use() {
    let Some(weasel) = compose(&answered(0.9)) else {
        panic!("an answered question must compose");
    };
    assert_eq!(weasel.label(), "weasel");
    let Some(ordinary) = compose(&answered(0.1)) else {
        panic!("an answered question must compose");
    };
    assert_eq!(ordinary.label(), "not_weasel");
    assert!((ordinary.score() - 0.9).abs() < 1e-9);
}

#[test]
fn compose_failed_call_records_nothing() {
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[test]
fn the_stream_scan_stays_the_substring_match_whatever_jev_would_say() {
    let mut scanner = DeltaScanner::new(&["robust".to_owned()]);
    assert_eq!(
        scanner
            .push("The design handles robustness concerns.")
            .as_deref(),
        Some("robust")
    );
}
