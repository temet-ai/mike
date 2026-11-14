use std::collections::BTreeMap;

use super::questions::questions;
use super::{accepted, compose, key, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::record::DecisionSubject;
use mike_launch::store::persistence::work_threads::criteria::ThreadCriteria;

fn thread(slug: &str) -> ThreadCriteria {
    ThreadCriteria {
        thread_slug: slug.to_owned(),
        title: format!("{slug} title"),
        criteria_text: format!("- {slug} is done."),
    }
}

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
fn questions_ask_one_yes_or_no_per_linked_thread() {
    let asked = questions(3);
    assert_eq!(asked.len(), 3);
    assert!(asked.contains_key(&key(2)));
}

#[test]
fn compose_accepts_only_a_clear_yes() {
    assert_eq!(compose(&answered(&[0.95, 0.6, 0.8]), 3), Some(vec![0, 2]));
    assert_eq!(compose(&answered(&[0.1, 0.2]), 2), Some(Vec::new()));
}

#[test]
fn compose_failed_or_missing_answer_accepts_nothing() {
    assert_eq!(compose(&Asked::failed_with("timeout"), 2), None);
    assert_eq!(compose(&answered(&[0.9]), 2), None);
}

#[test]
fn state_carries_every_ask_and_each_thread() {
    let threads = [thread("a"), thread("b")];
    let value = state(&threads, "  [ask] build a\n[correction] not b  ");
    assert_eq!(value["operator_asks"], "[ask] build a\n[correction] not b");
    assert_eq!(value["threads"][key(1)]["criteria"], "- b is done.");
    assert_eq!(value["threads"][key(0)]["title"], "a title");
}

#[tokio::test]
async fn accepted_without_a_decider_takes_no_thread() {
    let threads = [thread("a")];
    let got = accepted(None, &threads, "build a", DecisionSubject::default()).await;
    assert!(got.is_empty());
}
