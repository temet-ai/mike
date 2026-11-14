use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn stub(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn decider_for(server: &MockServer, persist: &PersistHandle) -> anyhow::Result<Decider> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Decider::with_mode(
        &client,
        persist,
        &SettingsOverlay::default(),
        POINT,
        DecisionMode::On,
    )
    .ok_or_else(|| anyhow::anyhow!("no decider"))
}

fn reply(first: i64, second: i64) -> serde_json::Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {
            format!("proposal_{first}"): {
                "type": "choice", "choice": "accept",
                "probabilities": {"accept": 0.9, "reject": 0.07, "duplicate": 0.03}
            },
            format!("proposal_{second}"): {
                "type": "choice", "choice": "reject",
                "probabilities": {"accept": 0.1, "reject": 0.85, "duplicate": 0.05}
            }
        },
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

fn keys(ids: &[i64]) -> Vec<String> {
    ids.iter().map(|id| format!("test_queue:{id}")).collect()
}

fn arrival_order(ids: &[i64], suggested: &BTreeMap<i64, f64>) -> Vec<i64> {
    let mut rows = ids.to_vec();
    order_by_suggestion(&mut rows, suggested, |id| (false, *id));
    rows
}

#[tokio::test]
async fn fill_on_stores_the_accept_chances_and_orders_the_likely_accept_first() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (rejected, accepted) = (900_021, 900_022);
    let server = stub(200, reply(accepted, rejected)).await;
    let decider = decider_for(&server, &persist)?;
    let items = [
        candidate(rejected, "proposal"),
        candidate(accepted, "proposal"),
    ];
    fill(&decider, &items).await;
    let stored = cached(keys(&[rejected, accepted]).iter().map(String::as_str));
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[&format!("test_queue:{accepted}")], 0.9);
    assert_eq!(stored[&format!("test_queue:{rejected}")], 0.1);
    let chances: BTreeMap<i64, f64> = BTreeMap::from([(accepted, 0.9), (rejected, 0.1)]);
    assert_eq!(
        arrival_order(&[rejected, accepted], &chances),
        vec![accepted, rejected]
    );
    let records = crate::llm::decisions::store(&persist)
        .export_decisions(chrono::DateTime::UNIX_EPOCH, Some(POINT), false)
        .await?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["outcome"], "ok");
    assert_eq!(records[0]["applied"], "ordered");
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn fill_failing_call_stores_no_guess_keeps_the_order_and_records_the_failure()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (first, second) = (900_023, 900_024);
    let server = stub(500, json!({"error": {"code": 500}})).await;
    let decider = decider_for(&server, &persist)?;
    let items = [candidate(first, "proposal"), candidate(second, "proposal")];
    fill(&decider, &items).await;
    let stored = cached(keys(&[first, second]).iter().map(String::as_str));
    assert!(stored.is_empty());
    assert_eq!(
        arrival_order(&[first, second], &stored_ids(&stored)),
        vec![first, second]
    );
    let records = crate::llm::decisions::store(&persist)
        .export_decisions(chrono::DateTime::UNIX_EPOCH, Some(POINT), false)
        .await?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["outcome"], "http_error");
    assert_eq!(records[0]["applied"], "none");
    persist.pool().close().await;
    db.cleanup().await
}

fn stored_ids(stored: &BTreeMap<String, f64>) -> BTreeMap<i64, f64> {
    stored
        .iter()
        .filter_map(|(key, chance)| Some((key.rsplit(':').next()?.parse().ok()?, *chance)))
        .collect()
}

fn candidate(id: i64, label: &'static str) -> Candidate {
    Candidate {
        key: format!("test_queue:{id}"),
        question_key: format!("proposal_{id}"),
        label,
        item: json!({ "id": id, "text": format!("item {id}") }),
    }
}

fn pick(key: &str, probabilities: &[(&str, f64)]) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: probabilities
            .iter()
            .map(|(k, p)| ((*k).to_owned(), *p))
            .collect(),
        confidence: None,
    }
}

#[test]
fn compose_reads_the_accept_probability_and_the_top_choice() {
    let items = [candidate(1, "proposal"), candidate(2, "proposal")];
    let group: Vec<&Candidate> = items.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        (
            "proposal_1".to_owned(),
            pick("accept", &[("accept", 0.8), ("reject", 0.2)]),
        ),
        (
            "proposal_2".to_owned(),
            pick("duplicate", &[("accept", 0.1), ("duplicate", 0.7)]),
        ),
    ]));
    assert_eq!(
        compose(&asked, &group),
        vec![
            ("test_queue:1".to_owned(), 0.8, "accept".to_owned()),
            ("test_queue:2".to_owned(), 0.1, "duplicate".to_owned())
        ]
    );
}

#[test]
fn compose_failed_answer_gives_no_guesses() {
    let items = [candidate(1, "proposal")];
    let group: Vec<&Candidate> = items.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &group).is_empty());
}

#[test]
fn questions_name_the_kind_of_item_being_asked_about() {
    let items = [candidate(7, "skill draft")];
    let group: Vec<&Candidate> = items.iter().collect();
    let asked = questions(&group);
    let Some(Question::Choice { instructions, .. }) = asked.get("proposal_7") else {
        panic!("expected a choice question for item 7");
    };
    assert!(instructions.contains("skill draft 7"));
    assert_eq!(state(&group)["proposals"][0]["id"], 7);
}

#[test]
fn label_for_compares_the_top_choice_with_the_operator_decision() {
    assert_eq!(label_for("accept", true), "right");
    assert_eq!(label_for("accept", false), "wrong");
    assert_eq!(label_for("duplicate", false), "right");
    assert_eq!(label_for("reject", true), "wrong");
}

#[test]
fn spawn_fill_without_a_decision_env_leaves_no_guess() {
    spawn_fill(None, vec![candidate(900_010, "proposal")]);
    assert!(cached(["test_queue:900010"]).is_empty());
    assert!(!FILLING.contains("test_queue:900010"));
}

#[test]
fn cached_returns_only_keys_that_have_a_guess() {
    GUESSES.insert(
        "test_queue:cached".to_owned(),
        Guess {
            accept: 0.6,
            top: "accept".to_owned(),
            record_id: None,
        },
    );
    let found = cached(["test_queue:cached", "test_queue:absent"]);
    assert_eq!(
        found,
        BTreeMap::from([("test_queue:cached".to_owned(), 0.6)])
    );
}
