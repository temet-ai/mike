use chrono::{Duration, Utc};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::wright::watch::logs::shape;

fn line(target: &str, message: &str, minutes_ago: i64) -> LogLine {
    LogLine {
        at: Utc::now() - Duration::minutes(minutes_ago),
        level: "ERROR".to_owned(),
        target: target.to_owned(),
        message: message.to_owned(),
        raw: message.to_owned(),
    }
}

fn groups(lines: &[LogLine]) -> Groups<'_> {
    let mut grouped = Groups::new();
    for line in lines {
        grouped
            .entry((line.target.clone(), shape(&line.message)))
            .or_default()
            .push(line);
    }
    grouped
}

fn answered(p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        SAME.to_owned(),
        Answer::Noul {
            noul: p,
            confidence: None,
        },
    )]))
}

fn reply(p: f64) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {SAME: {"type": "noul", "noul": p}},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

async fn stub(status: u16, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn decider(server: &MockServer, persist: &PersistHandle) -> anyhow::Result<Decider> {
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

fn two_wordings() -> Vec<LogLine> {
    vec![
        line(
            "mike::db",
            "connection to the database was refused by host 1",
            3,
        ),
        line(
            "mike::db",
            "connection to the database was refused by host 2",
            2,
        ),
        line(
            "mike::db",
            "the database refused the connection from host 3",
            1,
        ),
    ]
}

#[test]
fn similar_lines_that_share_most_words_are_candidates() {
    assert!(similar(
        "connection to the database was refused by host alpha",
        "the database refused the connection to host gamma",
    ));
    assert!(!similar("disk is full", "tls handshake failed for peer"));
}

#[test]
fn candidate_pairs_need_the_same_target_and_one_group_under_the_threshold() {
    let lines = [
        line("mike::db", "connection refused by host alpha", 1),
        line("mike::db", "connection refused by host beta", 1),
        line("mike::web", "connection refused by host alpha", 1),
    ];
    let listed: Vec<_> = groups(&lines).into_iter().collect();
    let pairs = candidate_pairs(&listed, 2);
    assert_eq!(pairs.len(), 1);
    assert_eq!(listed[pairs[0].0].0.0, listed[pairs[0].1].0.0);
    assert!(candidate_pairs(&listed, 1).is_empty());
}

#[test]
fn merged_joins_linked_groups_under_the_largest_groups_key() {
    let lines = two_wordings();
    let listed: Vec<_> = groups(&lines).into_iter().collect();
    assert_eq!(listed.len(), 2);
    let joined = merged(listed, &[(0, 1)]);
    assert_eq!(joined.len(), 1);
    let hits = joined.values().next().map(Vec::len);
    assert_eq!(hits, Some(3));
}

#[test]
fn compose_gives_the_probability_and_nothing_for_a_failed_call() {
    assert_eq!(compose(&answered(0.9)), Some(0.9));
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[tokio::test]
async fn merge_on_a_same_problem_answer_joins_the_groups() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(0.95)).await;
    let decider = decider(&server, &persist)?;
    let lines = two_wordings();
    let joined = merge(&decider, groups(&lines), 3).await;
    assert_eq!(joined.len(), 1);
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn merge_with_a_failing_client_leaves_the_groups_as_the_digit_rule_made_them()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let lines = two_wordings();
    let joined = merge(&decider, groups(&lines), 3).await;
    assert_eq!(joined.len(), 2);
    persist.pool().close().await;
    db.cleanup().await
}
