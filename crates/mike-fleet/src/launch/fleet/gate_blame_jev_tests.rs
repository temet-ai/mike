use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::launch::fleet::decisions::test_support::{
    DecisionsFixture, choice_body, jev_decider, stub_jev,
};
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;

fn change(files: &[&str]) -> Change {
    Change {
        files: files.iter().map(|f| (*f).to_owned()).collect(),
        ..Change::default()
    }
}

fn commits() -> Vec<(String, String)> {
    vec![
        ("aaaaaaaaaaaa".to_owned(), "worker-T1".to_owned()),
        ("bbbbbbbbbbbb".to_owned(), "worker-T2".to_owned()),
    ]
}

fn picked(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        WHOSE.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

#[test]
fn compose_a_candidate_key_gives_that_candidate_with_its_probability() {
    assert_eq!(
        compose(&picked("c2", 0.9), 2),
        Some(Pick::Candidate {
            index: 1,
            probability: 0.9
        })
    );
}

#[test]
fn compose_none_gives_nobody() {
    assert_eq!(
        compose(&picked("none", 0.7), 2),
        Some(Pick::Nobody { probability: 0.7 })
    );
}

#[test]
fn compose_a_key_outside_the_list_gives_nothing() {
    assert_eq!(compose(&picked("c9", 0.9), 2), None);
}

#[test]
fn compose_a_failed_call_gives_nothing() {
    assert_eq!(compose(&Asked::failed_with("timeout"), 2), None);
}

#[test]
fn candidates_of_keeps_the_newest_twelve_and_their_lexical_evidence() {
    let many: Vec<(String, String)> = (0..15)
        .map(|n| (format!("{n:040}"), "worker".to_owned()))
        .collect();
    let changes: Vec<Change> = (0..15).map(|_| change(&["a.rs"])).collect();
    let found = vec![(14, Evidence::Touches("a.rs".to_owned()))];
    let candidates = candidates_of(&many, &changes, &found);
    assert_eq!(candidates.len(), 12);
    assert!(candidates[11].lexical.is_some());
    assert!(candidates[0].lexical.is_none());
}

#[test]
fn state_lists_each_candidate_with_its_files_and_the_output_tail() {
    let changes = vec![change(&["a.rs", "b.rs"]), change(&["c.rs"])];
    let commits = commits();
    let candidates = candidates_of(&commits, &changes, &[]);
    let state = state("error in a.rs:3", &candidates);
    assert_eq!(state["failing_output_tail"], "error in a.rs:3");
    assert_eq!(state["commits_oldest_first"][0]["id"], "c1");
    assert_eq!(
        state["commits_oldest_first"][0]["files"],
        json!(["a.rs", "b.rs"])
    );
    assert_eq!(state["commits_oldest_first"][1]["seat"], "worker-T2");
}

#[test]
fn blame_of_a_candidate_names_one_suspect_judged_by_jev() {
    let changes = vec![change(&["a.rs"]), change(&["c.rs"])];
    let commits = commits();
    let candidates = candidates_of(&commits, &changes, &[]);
    let pick = Pick::Candidate {
        index: 1,
        probability: 0.8,
    };
    let Blame::Suspects(found) = blame_of(&pick, &candidates, 2) else {
        panic!("a suspect is named");
    };
    assert_eq!(found[0].seat, "worker-T2");
    assert_eq!(found[0].evidence, Evidence::Judged { percent: 80 });
}

#[tokio::test]
async fn judged_on_an_answered_choice_names_the_picked_commit() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, choice_body(WHOSE, "c2", &["c1", "none"])).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let changes = vec![change(&["a.rs"]), change(&["c.rs"])];
    let commits = commits();
    let candidates = candidates_of(&commits, &changes, &[]);
    let found = judged(&decider, &candidates, "boom", 2).await;
    let Some(Blame::Suspects(suspects)) = found else {
        panic!("a suspect is named");
    };
    assert_eq!(suspects[0].commit, "bbbbbbbbbbbb");
    fx.finish().await
}

#[tokio::test]
async fn judged_with_a_failing_client_gives_nothing_so_the_lexical_rule_decides()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(500, json!({})).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let changes = vec![change(&["a.rs"])];
    let commits = commits();
    let candidates = candidates_of(&commits[..1], &changes, &[]);
    assert_eq!(judged(&decider, &candidates, "boom", 1).await, None);
    fx.finish().await
}

#[tokio::test]
async fn judged_in_shadow_records_and_changes_nothing() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, choice_body(WHOSE, "c1", &["none"])).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::Shadow)?;
    let changes = vec![change(&["a.rs"])];
    let commits = commits();
    let candidates = candidates_of(&commits[..1], &changes, &[]);
    assert_eq!(judged(&decider, &candidates, "boom", 1).await, None);
    fx.finish().await
}
