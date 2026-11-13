use chrono::Utc;

use super::directives::{Polarity, entries_for, extract};
use crate::store::persistence::dossier_entries::DirectiveSourceRow;

fn classes(text: &str) -> Vec<(&'static str, Polarity)> {
    let mut found: Vec<(&'static str, Polarity)> = extract(text)
        .into_iter()
        .map(|m| (m.class, m.polarity))
        .collect();
    found.dedup();
    found
}

#[test]
fn commit_allow_and_forbid() {
    assert_eq!(
        classes("go ahead and commit"),
        vec![("commit", Polarity::Allow)]
    );
    assert_eq!(
        classes("commit when the tests pass"),
        vec![("commit", Polarity::Allow)]
    );
    assert_eq!(
        classes("don't commit yet"),
        vec![("commit", Polarity::Forbid)]
    );
    assert_eq!(
        classes("fix it without committing"),
        vec![("commit", Polarity::Forbid)]
    );
}

#[test]
fn a_forbid_suppresses_the_overlapping_allow_of_its_class() {
    assert_eq!(
        classes("Do not commit it"),
        vec![("commit", Polarity::Forbid)]
    );
}

#[test]
fn push_merge_and_deploy_follow_the_commit_shape() {
    assert_eq!(classes("push when green"), vec![("push", Polarity::Allow)]);
    assert_eq!(
        classes("never push to main"),
        vec![("push", Polarity::Forbid)]
    );
    assert_eq!(classes("then merge"), vec![("merge", Polarity::Allow)]);
    assert_eq!(
        classes("finish without merging"),
        vec![("merge", Polarity::Forbid)]
    );
    assert_eq!(
        classes("feel free to deploy"),
        vec![("deploy", Polarity::Allow)]
    );
    assert_eq!(
        classes("no need to deploy"),
        vec![("deploy", Polarity::Forbid)]
    );
    assert_eq!(
        classes("land it without deploying"),
        vec![("deploy", Polarity::Forbid)]
    );
}

#[test]
fn pull_request_allow_and_forbid() {
    assert_eq!(classes("open a PR for this"), vec![("pr", Polarity::Allow)]);
    let forbid = classes("don't open a pull request");
    assert!(forbid.contains(&("pr", Polarity::Forbid)));
    assert!(!forbid.contains(&("pr", Polarity::Allow)));
}

#[test]
fn proceed_and_ask_first() {
    assert_eq!(classes("just do it"), vec![("proceed", Polarity::Allow)]);
    assert_eq!(classes("stop asking"), vec![("proceed", Polarity::Allow)]);
    assert_eq!(
        classes("no need to ask"),
        vec![("proceed", Polarity::Allow)]
    );
    assert_eq!(
        classes("you don't need my permission"),
        vec![("proceed", Polarity::Allow)]
    );
    assert_eq!(
        classes("carry on without asking"),
        vec![("proceed", Polarity::Allow)]
    );
    assert_eq!(
        classes("ask me first"),
        vec![("ask_first", Polarity::Forbid)]
    );
    assert_eq!(
        classes("check with me before"),
        vec![("ask_first", Polarity::Forbid)]
    );
    assert_eq!(
        classes("wait for my go"),
        vec![("ask_first", Polarity::Forbid)]
    );
}

#[test]
fn text_without_a_directive_yields_nothing() {
    assert!(extract("please refactor the store and add tests").is_empty());
}

#[test]
fn spans_are_character_offsets_into_the_prompt() {
    let found = extract("héllo, please commit");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].span, (7, 20));
}

#[test]
fn entries_carry_provenance_and_one_row_per_class_and_polarity() {
    let source = DirectiveSourceRow {
        turn_id: 5,
        session_id: 9,
        start_request_id: 77,
        user_prompt: "please commit, then commit again, but don't push".to_owned(),
        created_at: Utc::now(),
    };
    let entries = entries_for(std::slice::from_ref(&source));
    let keys: Vec<&str> = entries.iter().map(|e| e.dedup_key.as_str()).collect();
    assert_eq!(
        keys,
        vec!["request:77:commit:allow", "request:77:push:forbid"]
    );
    assert!(
        entries
            .iter()
            .all(|e| e.source_ref == "requests.requests:77")
    );
    assert!(entries.iter().all(|e| e.kind == "directive"));
    assert_eq!(entries[0].data["polarity"], "allow");
}
