use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{key, questions};
use super::state::state;
use super::{Candidate, is_held, remember_holds};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn lesson(id: i64, text: &str) -> Candidate<'_> {
    Candidate {
        id,
        kind: "correction",
        text,
    }
}

fn answered(pairs: &[(i64, f64)]) -> Asked {
    Asked::answered(
        pairs
            .iter()
            .map(|(id, p)| {
                (
                    key(*id),
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
fn compose_splits_lessons_at_the_floor() {
    let supply = compose(&answered(&[(1, 0.9), (2, 0.2), (3, 0.5)]), &[1, 2, 3]);
    let Some(supply) = supply else {
        panic!("no supply");
    };
    assert_eq!(supply.keep, vec![1, 3]);
    assert_eq!(supply.hold, vec![2]);
}

#[test]
fn compose_failed_ask_is_none_so_every_lesson_is_supplied_as_today() {
    assert!(compose(&Asked::failed_with("timeout"), &[1, 2]).is_none());
}

#[test]
fn compose_missing_answer_is_none() {
    assert!(compose(&answered(&[(1, 0.9)]), &[1, 2]).is_none());
}

#[test]
fn questions_ask_one_question_per_lesson() {
    let a = lesson(7, "use scripts/cx");
    let b = lesson(9, "never cargo clean");
    let asked = questions(&[a, b]);
    assert!(asked.contains_key("lesson_7") && asked.contains_key("lesson_9"));
}

#[test]
fn state_caps_the_lesson_text_and_keeps_the_context_end() {
    let body = "w".repeat(5_000);
    let long = lesson(1, &body);
    let context = format!("{}NOW EDITING THE PARSER", "c".repeat(4_000));
    let s = state(&context, &[long]);
    assert_eq!(s["lessons"]["1"]["text"].as_str().map(str::len), Some(700));
    assert!(
        s["session_context"]
            .as_str()
            .unwrap_or_default()
            .ends_with("NOW EDITING THE PARSER")
    );
}

#[test]
fn held_lessons_are_remembered_per_session() {
    remember_holds("supply-floor-test-session", &[41]);
    assert!(is_held("supply-floor-test-session", 41));
    assert!(!is_held("supply-floor-test-session", 42));
    assert!(!is_held("another-session", 41));
}
