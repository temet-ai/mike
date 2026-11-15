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
    wordings_on("mike::db")
}

fn wordings_on(target: &str) -> Vec<LogLine> {
    vec![
        line(
            target,
            "connection to the database was refused by host 1",
            3,
        ),
        line(
            target,
            "connection to the database was refused by host 2",
            2,
        ),
        line(target, "the database refused the connection from host 3", 1),
    ]
}

fn slow(statement: &str) -> String {
    format!("{SLOW_STATEMENT_PREFIX} summary={statement}")
}

#[test]
fn distinct_drops_only_the_shared_slow_statement_prefix() {
    let (left, right) = (
        slow("SELECT id FROM requests"),
        slow("UPDATE sessions SET seen"),
    );
    let (a, b) = distinct(&left, &right);
    assert_eq!(
        (a, b),
        (
            "summary=SELECT id FROM requests",
            "summary=UPDATE sessions SET seen"
        )
    );
    let (a, b) = distinct(
        "db error: connection refused: host a",
        "db error: connection refused: host b",
    );
    assert_eq!(
        (a, b),
        (
            "db error: connection refused: host a",
            "db error: connection refused: host b"
        )
    );
    let (a, b) = distinct("disk is full", "tls handshake failed");
    assert_eq!((a, b), ("disk is full", "tls handshake failed"));
}

#[test]
fn slow_statements_that_differ_in_their_sql_are_not_candidate_pairs() {
    let lines = [
        line("sqlx::query", &slow("SELECT id FROM requests"), 1),
        line("sqlx::query", &slow("UPDATE sessions SET seen"), 1),
        line(
            "sqlx::query",
            &slow("SELECT id FROM requests WHERE live"),
            1,
        ),
    ];
    let listed: Vec<_> = groups(&lines).into_iter().collect();
    let pairs = candidate_pairs(&listed, 2);
    assert_eq!(pairs.len(), 1);
    let texts: Vec<&str> = [pairs[0].0, pairs[0].1]
        .iter()
        .map(|at| listed[*at].0.1.as_str())
        .collect();
    assert!(
        texts
            .iter()
            .all(|text| text.contains("SELECT id FROM requests"))
    );
}

#[tokio::test]
async fn merge_uses_a_remembered_verdict_without_asking_again() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let lines = wordings_on("mike::memoised");
    let listed: Vec<_> = groups(&lines).into_iter().collect();
    remember(memo_key(&listed[0].0, &listed[1].0), 0.93);
    let joined = merge(&decider, groups(&lines), 3).await;
    assert_eq!(joined.len(), 1);
    let asked = server
        .received_requests()
        .await
        .map_or(0, |sent| sent.len());
    assert_eq!(asked, 0);
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn merge_asks_a_pair_once_and_remembers_the_answer() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(0.95)).await;
    let decider = decider(&server, &persist)?;
    let lines = wordings_on("mike::asked_once");
    assert_eq!(merge(&decider, groups(&lines), 3).await.len(), 1);
    assert_eq!(merge(&decider, groups(&lines), 3).await.len(), 1);
    let asked = server
        .received_requests()
        .await
        .map_or(0, |sent| sent.len());
    assert_eq!(asked, 1);
    persist.pool().close().await;
    db.cleanup().await
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
fn question_names_both_messages_and_states_the_criteria() {
    let asked = questions();
    let Some(Question::Noul {
        instructions,
        criteria,
    }) = asked.get(SAME)
    else {
        panic!("same is not a noul");
    };
    assert_eq!(
        instructions,
        "Do `line_a.message` and `line_b.message` report the same problem?"
    );
    assert_eq!(
        criteria.yes,
        "The same statement or error; only names, numbers or identifiers differ"
    );
    assert_eq!(
        criteria.no,
        "Different SQL statements, tables or error kinds"
    );
}

#[test]
fn applied_for_records_merged_kept_apart_and_undecided_pairs() {
    assert_eq!(applied_for(Some(0.9), true), "chosen");
    assert_eq!(applied_for(Some(0.2), false), "skipped");
    assert_eq!(applied_for(Some(0.3), false), "none");
    assert_eq!(applied_for(Some(0.5), false), "none");
    assert_eq!(applied_for(Some(0.85), false), "none");
    assert_eq!(applied_for(None, false), "none");
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
    let lines = wordings_on("mike::joined");
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
    let lines = wordings_on("mike::failing");
    let joined = merge(&decider, groups(&lines), 3).await;
    assert_eq!(joined.len(), 2);
    persist.pool().close().await;
    db.cleanup().await
}
