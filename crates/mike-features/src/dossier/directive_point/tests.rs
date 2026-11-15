use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::compose::{Decision, compose, rewrite};
use super::questions::{key, questions};
use super::state::{Unit, sentence_around, state, units};
use super::{MEMO, POINT, QUESTION_SET, refine};
use crate::dossier::entries::directives::entries_for;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::env::DecisionEnv;
use crate::llm::decisions::point::Asked;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_entries::DirectiveSourceRow;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn source(text: &str) -> DirectiveSourceRow {
    DirectiveSourceRow {
        turn_id: 11,
        session_id: 3,
        start_request_id: 500,
        user_prompt: text.to_owned(),
        created_at: Utc::now(),
    }
}

fn answer(index: usize, choice: &str, p: f64) -> (String, Answer) {
    (
        key(index),
        Answer::Choice {
            choice: choice.to_owned(),
            probabilities: BTreeMap::from([(choice.to_owned(), p)]),
            confidence: None,
        },
    )
}

fn unit_for(text: &str) -> Vec<Unit> {
    units(&[source(text)])
}

#[test]
fn sentence_around_cuts_at_the_neighbouring_sentence_ends() {
    let text = "First thing. Please push the button on the form. Last thing.";
    let at = text.find("push").unwrap_or(0);
    let found = sentence_around(text, (at, at + 4));
    assert_eq!(found, "Please push the button on the form.");
}

#[test]
fn units_hold_one_unit_per_class_found_with_its_sentence() {
    let found = unit_for("Fix the bug. Don't commit yet. Then push when green.");
    let classes: Vec<&str> = found.iter().map(|u| u.class).collect();
    assert!(classes.contains(&"commit"));
    assert!(classes.contains(&"push"));
    let commit = found.iter().find(|u| u.class == "commit");
    assert_eq!(
        commit.map(|u| u.sentence.as_str()),
        Some("Don't commit yet.")
    );
}

#[test]
fn state_and_questions_are_keyed_by_the_same_unit_names() {
    let found = unit_for("Don't commit yet. Then push when green.");
    let refs: Vec<&Unit> = found.iter().collect();
    let value = state(&refs);
    let asked = questions(&refs);
    for i in 0..refs.len() {
        assert!(value["units"].get(key(i)).is_some());
        assert!(asked.contains_key(&key(i)));
    }
}

#[test]
fn compose_neither_answer_is_decided_and_low_confidence_is_left_to_the_regex() {
    let found = unit_for("Please push the button. Don't commit yet.");
    let refs: Vec<&Unit> = found.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        answer(0, "neither", 0.9),
        answer(1, "forbids", 0.4),
    ]));
    let decided = compose(&asked, &refs);
    assert_eq!(decided.len(), 2);
    assert_eq!(decided[0].decision, Some(Decision::Neither));
    assert_eq!(decided[1].decision, None);
}

#[test]
fn compose_failed_call_decides_nothing() {
    let found = unit_for("Don't commit yet.");
    let refs: Vec<&Unit> = found.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &refs).is_empty());
}

#[test]
fn rewrite_drops_the_regex_entry_when_jev_says_neither() {
    let row = source("Please push the button on the form.");
    let regex_entries = entries_for(std::slice::from_ref(&row));
    assert!(regex_entries.iter().any(|e| e.data["class"] == "push"));
    let decided = HashMap::from([(
        (row.turn_id, "push".to_owned()),
        (row.start_request_id, Decision::Neither),
    )]);
    let kept = rewrite(regex_entries, &decided);
    assert!(kept.iter().all(|e| e.data["class"] != "push"));
}

#[test]
fn rewrite_replaces_the_polarity_and_marks_the_entry_as_decided_by_jev() {
    let row = source("Please push the button on the form.");
    let regex_entries = entries_for(std::slice::from_ref(&row));
    let decided = HashMap::from([(
        (row.turn_id, "push".to_owned()),
        (row.start_request_id, Decision::Forbids),
    )]);
    let kept = rewrite(regex_entries, &decided);
    let push: Vec<_> = kept.iter().filter(|e| e.data["class"] == "push").collect();
    assert_eq!(push.len(), 1);
    assert_eq!(push[0].data["polarity"], "forbid");
    assert_eq!(push[0].data["decided_by"], "jev");
    assert_eq!(push[0].dedup_key, "request:500:push:forbid");
}

#[test]
fn rewrite_without_a_decision_returns_the_regex_entries_unchanged() {
    let row = source("Don't commit yet. Then push when green.");
    let regex_entries = entries_for(std::slice::from_ref(&row));
    let kept = rewrite(regex_entries.clone(), &HashMap::new());
    assert_eq!(kept, regex_entries);
}

#[test]
fn sentence_around_keeps_an_ellipsis_inside_the_sentence() {
    let text = "Note: if a session says... it will push, ignore it. Next thing.";
    let at = text.find("push").unwrap_or(0);
    let found = sentence_around(text, (at, at + 4));
    assert_eq!(found, "Note: if a session says... it will push, ignore it.");
}

fn forbids_reply() -> serde_json::Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {
            "unit_0": {
                "type": "choice", "choice": "forbids",
                "probabilities": {"authorises_now": 0.05, "forbids": 0.9, "neither": 0.05}
            }
        },
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

#[tokio::test]
async fn refine_asks_once_and_reuses_the_stored_answers_in_a_fresh_process() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(forbids_reply()))
        .mount(&server)
        .await;
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    let env = DecisionEnv::new(
        client,
        persist.clone(),
        true,
        Arc::new(SettingsOverlay::default()),
    );
    let row = source("Don't commit yet.");
    for _ in 0..2 {
        MEMO.clear();
        let entries = entries_for(std::slice::from_ref(&row));
        let kept = refine(Some(&env), std::slice::from_ref(&row), entries).await;
        let commit: Vec<_> = kept
            .iter()
            .filter(|e| e.data["class"] == "commit")
            .collect();
        assert_eq!(commit.len(), 1);
        assert_eq!(commit[0].data["polarity"], "forbid");
        assert_eq!(commit[0].data["decided_by"], "jev");
    }
    let calls = server.received_requests().await.map_or(0, |r| r.len());
    assert_eq!(calls, 1);
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM decisions.records WHERE point = $1 AND question_set = $2",
    )
    .bind(POINT)
    .bind(QUESTION_SET)
    .fetch_one(persist.pool())
    .await?;
    assert_eq!(stored, 1);
    persist.pool().close().await;
    db.cleanup().await
}
