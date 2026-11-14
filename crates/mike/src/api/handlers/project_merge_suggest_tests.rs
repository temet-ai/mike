use chrono::{Duration as Span, Utc};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn project(path: &str, name: &str, remote: Option<&str>, age_days: i64) -> MergeCandidate {
    MergeCandidate {
        id: 1,
        project_uuid: Uuid::new_v4(),
        project_path: path.to_owned(),
        display_name: name.to_owned(),
        last_seen: Utc::now() - Span::days(age_days),
        origin_urls: remote.map(|r| vec![r.to_owned()]).unwrap_or_default(),
    }
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

#[test]
fn may_be_the_same_a_worktree_folder_of_a_repository_is_a_candidate() {
    let main = project("/src/app", "app", None, 0);
    let worktree = project("/src/app-wt-1", "app-wt-1", None, 3);
    assert!(may_be_the_same(&main, &worktree));
}

#[test]
fn may_be_the_same_a_shared_git_remote_is_a_candidate() {
    let a = project("/home/a/code", "code", Some("github.com/o/r"), 0);
    let b = project("/work/other", "other", Some("github.com/o/r"), 1);
    assert!(may_be_the_same(&a, &b));
}

#[test]
fn may_be_the_same_unrelated_projects_are_not_candidates() {
    let a = project("/src/app", "app", None, 0);
    let b = project("/src/billing", "billing", None, 1);
    assert!(!may_be_the_same(&a, &b));
}

#[test]
fn pairs_name_the_older_project_as_the_source() {
    let newer = project("/src/app", "app", None, 0);
    let older = project("/src/app-wt-1", "app-wt-1", None, 5);
    let candidates = [older.clone(), newer.clone()];
    let found = pairs(&candidates);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source.project_path, "/src/app-wt-1");
    assert_eq!(found[0].target.project_path, "/src/app");
}

#[test]
fn compose_an_answered_noul_gives_the_probability_and_a_failed_call_nothing() {
    assert_eq!(compose(&answered(0.8)), Some(0.8));
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[tokio::test]
async fn suggest_on_a_high_answer_proposes_the_merge_without_making_it() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(200, reply(0.9)).await;
    let decider = decider(&server, &persist)?;
    let candidates = [
        project("/src/billing", "billing", None, 0),
        project("/src/billing-wt-1", "billing-wt-1", None, 4),
    ];
    let found = suggest(&decider, &candidates).await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source_path, "/src/billing-wt-1");
    assert_eq!(found[0].target_path, "/src/billing");
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn suggest_with_a_failing_client_proposes_nothing() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    let candidates = [
        project("/src/ledger", "ledger", None, 0),
        project("/src/ledger-wt-1", "ledger-wt-1", None, 4),
    ];
    assert!(suggest(&decider, &candidates).await.is_empty());
    persist.pool().close().await;
    db.cleanup().await
}
