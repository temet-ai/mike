use std::collections::BTreeMap;

use super::asks::{Asks, HEAD_CHARS, TAIL_CHARS, first_ask, fit};
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
    let asks = Asks {
        first: "  [initial] build a and b  ".to_owned(),
        later: "  [ask] build a\n[correction] not b  ".to_owned(),
    };
    let value = state(&threads, &asks);
    assert_eq!(value["first_ask"], "[initial] build a and b");
    assert_eq!(value["later_asks"], "[ask] build a\n[correction] not b");
    assert_eq!(value["threads"][key(1)]["criteria"], "- b is done.");
    assert_eq!(value["threads"][key(0)]["title"], "a title");
}

#[test]
fn fit_keeps_the_initial_ask_and_the_newest_asks_when_over_budget() {
    let middle = "m".repeat(9_000);
    let asks = format!(
        "[initial] build the ledger {}\n[clarification] {middle}\n[correction] not b, the end",
        "i".repeat(2_000)
    );
    let fitted = fit(&asks);
    assert!(fitted.starts_with("[initial] build the ledger iii"));
    assert!(fitted.ends_with("[correction] not b, the end"));
    assert!(fitted.chars().count() <= HEAD_CHARS + TAIL_CHARS + 10);
}

#[test]
fn fit_drops_a_pasted_skill_body_so_it_cannot_push_the_initial_ask_out() {
    let body = "x".repeat(8_000);
    let asks = format!(
        "[initial] build a\n[clarification] <command-name>/fleet</command-name>\n{body}\n[correction] not b"
    );
    let fitted = fit(&asks);
    assert_eq!(
        fitted,
        "[initial] build a\n[clarification]\n[pasted text dropped]\n[correction] not b"
    );
}

#[test]
fn fit_drops_a_skill_header_message_and_a_json_fleet_definition() {
    let asks = "[initial] build a\n[clarification] # /fleet\nWrite the definition\nmore text\n[clarification] go on\n{\"seats\": [1, 2],\n \"tasks\": []}\n[correction] not b";
    let fitted = fit(asks);
    assert_eq!(
        fitted,
        "[initial] build a\n[clarification]\n[pasted text dropped]\n[clarification] go on\n[pasted text dropped]\n[correction] not b"
    );
    assert!(!fitted.contains("seats"));
}

#[test]
fn fit_leaves_a_short_plain_ask_list_unchanged() {
    let asks = "[initial] build a {not json}\n[correction] not b";
    assert_eq!(fit(asks), asks);
}

#[tokio::test]
async fn accepted_without_a_decider_takes_no_thread() {
    let threads = [thread("a")];
    let asks = Asks {
        first: "build a".to_owned(),
        later: String::new(),
    };
    let got = accepted(None, &threads, &asks, DecisionSubject::default()).await;
    assert!(got.is_empty());
}

#[test]
fn first_ask_drops_a_pasted_skill_body_and_keeps_the_ask() {
    let first = format!("[initial] Finish #221.\n# /fleet\n{}", "x".repeat(3_000));
    assert_eq!(
        first_ask(&first),
        "[initial] Finish #221.\n[pasted text dropped]"
    );
}
