use std::collections::BTreeMap;

use super::*;
use crate::llm::decisions::Answer;

fn answered(describes: f64, unmentioned: f64) -> Asked {
    Asked::answered(BTreeMap::from([
        (
            DESCRIBES.to_owned(),
            Answer::Noul {
                noul: describes,
                confidence: None,
            },
        ),
        (
            UNMENTIONED.to_owned(),
            Answer::Noul {
                noul: unmentioned,
                confidence: None,
            },
        ),
    ]))
}

#[test]
fn compose_a_matching_message_gives_no_warning() {
    assert!(compose(&answered(0.95, 0.05)).warnings.is_empty());
}

#[test]
fn compose_a_message_about_something_else_names_the_gap() {
    let verdict = compose(&answered(0.1, 0.1));
    assert_eq!(verdict.warnings.len(), 1);
    assert!(verdict.warnings[0].contains("does not seem to describe"));
}

#[test]
fn compose_unmentioned_work_names_the_gap() {
    let verdict = compose(&answered(0.9, 0.9));
    assert_eq!(verdict.warnings.len(), 1);
    assert!(verdict.warnings[0].contains("does not mention"));
}

#[test]
fn compose_a_failed_call_gives_no_warning() {
    let verdict = compose(&Asked::failed_with("timeout"));
    assert!(verdict.warnings.is_empty());
}

#[test]
fn state_lists_the_files_and_cuts_the_diff_at_the_cap() {
    let diff = "x".repeat(DIFF_CAP_BYTES + 10);
    let value = state("Fix the parser", "3\t1\tsrc/a.rs\n-\t-\tlogo.png", &diff);
    assert_eq!(value["files"][0]["path"], "src/a.rs");
    assert_eq!(value["files"][1]["added"], "-");
    assert_eq!(
        value["unified_diff"].as_str().map(str::len),
        Some(DIFF_CAP_BYTES)
    );
    assert_eq!(value["unified_diff_cut"], true);
}

#[test]
fn questions_ask_two_noul_questions() {
    assert_eq!(questions().len(), 2);
}
