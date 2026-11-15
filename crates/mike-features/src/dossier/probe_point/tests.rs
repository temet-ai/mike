use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::compose::compose;
use super::questions::{key, questions};
use super::state::{Candidate, Reason, state};
use super::{POINT, QUESTION_SET, VERDICTS, fill, pending, remember, verdict};
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::env::DecisionEnv;
use crate::llm::decisions::point::Asked;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn candidate(session_id: i64, prompt: &str, reason: Reason) -> Candidate {
    Candidate {
        session_id,
        first_prompt: prompt.to_owned(),
        project_path: Some("/work/app".to_owned()),
        origin: "direct",
        turn_count: 1,
        reason,
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
fn questions_ask_one_noul_per_candidate_session() {
    let a = candidate(1, "Reply with exactly OK", Reason::RegexMatch);
    let b = candidate(2, "hi", Reason::ShortSingleTurn);
    let asked = questions(&[&a, &b]);
    assert_eq!(asked.len(), 2);
    assert!(asked.contains_key(&key(1)));
    assert!(asked.contains_key(&key(2)));
}

#[test]
fn state_carries_the_prompt_and_whether_the_pattern_matched() {
    let a = candidate(1, "Reply with exactly OK", Reason::RegexMatch);
    let value = state(&[&a]);
    assert_eq!(
        value["sessions"][key(1)]["probe_pattern_matched"],
        serde_json::json!(true)
    );
    assert_eq!(
        value["sessions"][key(1)]["first_prompt"],
        serde_json::json!("Reply with exactly OK")
    );
}

#[test]
fn compose_marks_a_session_a_probe_at_or_above_one_half() {
    let a = candidate(1, "x", Reason::RegexMatch);
    let b = candidate(2, "y", Reason::RegexMatch);
    let found = compose(&answered(&[(1, 0.9), (2, 0.2)]), &[&a, &b]);
    assert_eq!(found.len(), 2);
    assert!(found[0].is_probe);
    assert!(!found[1].is_probe);
    assert!((found[1].score() - 0.8).abs() < 1e-9);
}

#[test]
fn compose_failed_call_gives_no_verdicts() {
    let a = candidate(1, "x", Reason::RegexMatch);
    assert!(compose(&Asked::failed_with("timeout"), &[&a]).is_empty());
}

#[test]
fn remember_then_verdict_returns_the_answer_and_pending_skips_known_sessions() {
    let id = 987_654_001;
    remember(&[super::Verdict {
        session_id: id,
        is_probe: true,
        probability: 0.9,
    }]);
    assert_eq!(verdict(id), Some(true));
    let left = pending(vec![
        candidate(id, "x", Reason::RegexMatch),
        candidate(id + 1, "y", Reason::RegexMatch),
    ]);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].session_id, id + 1);
}

#[test]
fn state_carries_the_origin_the_cwd_and_the_turn_count() {
    let mut a = candidate(1, "rename a to first", Reason::ShortSingleTurn);
    a.project_path = None;
    a.origin = "mike_launch";
    a.turn_count = 2;
    let value = state(&[&a]);
    assert_eq!(value["sessions"][key(1)]["cwd"], serde_json::Value::Null);
    assert_eq!(value["sessions"][key(1)]["origin"], json!("mike_launch"));
    assert_eq!(value["sessions"][key(1)]["turn_count"], json!(2));
}

#[test]
fn questions_name_scripted_test_tasks_as_probes() {
    let a = candidate(1, "rename a to first", Reason::ShortSingleTurn);
    let text = serde_json::to_string(&questions(&[&a])).unwrap_or_default();
    assert!(text.contains("scripted"));
}

#[tokio::test]
async fn fill_asks_once_and_reuses_the_stored_answers_in_a_fresh_process() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let id = 987_654_101;
    let server = MockServer::start().await;
    let reply = json!({
        "model": "typesafe/jev-1.14",
        "answers": { key(id): {"type": "noul", "noul": 0.9} },
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    });
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply))
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
    for _ in 0..2 {
        VERDICTS.remove(&id);
        fill(
            &env,
            vec![candidate(id, "rename a to first", Reason::ShortSingleTurn)],
        )
        .await;
        assert_eq!(verdict(id), Some(true));
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
