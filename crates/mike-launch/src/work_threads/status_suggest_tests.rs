use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};
use crate::store::persistence::work_threads::{CheckpointInput, MetaUpdate, StateFields};

fn answered(finished: f64, parked: f64) -> Asked {
    let noul = |p: f64| Answer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        (FINISHED.to_owned(), noul(finished)),
        (PARKED.to_owned(), noul(parked)),
    ]))
}

fn reply(finished: f64, parked: f64) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {
            FINISHED: {"type": "noul", "noul": finished},
            PARKED: {"type": "noul", "noul": parked}
        },
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

async fn checkpointed(persist: &PersistHandle) -> anyhow::Result<i64> {
    let project = seed_project(persist.pool(), "/src/threads").await?;
    let files: Vec<String> = Vec::new();
    persist
        .launch()
        .checkpoint_work_thread(
            project,
            &CheckpointInput {
                slug: "frontend-motion",
                tldr: "All motion tokens shipped and verified.",
                state: StateFields {
                    where_we_are: "Everything shipped and verified on all machines.",
                    next_up: "",
                    blockers: "",
                    open_questions: "",
                    decisions: "",
                    files: &files,
                },
                session_uuid: None,
                commit_sha: None,
                contribution: None,
                meta: MetaUpdate {
                    title: Some("Frontend motion"),
                    status: Some("active"),
                    ..MetaUpdate::default()
                },
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(project)
}

async fn findings(persist: &PersistHandle, project: i64) -> anyhow::Result<usize> {
    let detail = persist
        .launch()
        .get_work_thread(project, "frontend-motion", HistoryPage::everything())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(detail
        .history
        .iter()
        .filter(|e| e.kind == "finding")
        .count())
}

#[test]
fn compose_a_confident_finished_answer_suggests_done() {
    assert_eq!(
        compose(&answered(0.9, 0.1)),
        Some(Suggestion {
            status: "done",
            probability: 0.9
        })
    );
}

#[test]
fn compose_a_confident_parked_answer_suggests_parked() {
    assert_eq!(
        compose(&answered(0.2, 0.85)),
        Some(Suggestion {
            status: "parked",
            probability: 0.85
        })
    );
}

#[test]
fn compose_unsure_answers_and_failed_calls_suggest_nothing() {
    assert_eq!(compose(&answered(0.5, 0.4)), None);
    assert_eq!(compose(&Asked::failed_with("timeout")), None);
}

#[tokio::test]
async fn run_on_a_finished_looking_thread_writes_a_finding_and_leaves_the_status()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let project = checkpointed(&persist).await?;
    let server = stub(200, reply(0.95, 0.05)).await;
    let decider = decider(&server, &persist)?;
    run(&decider, &persist, project, "frontend-motion").await;
    assert_eq!(findings(&persist, project).await?, 1);
    run(&decider, &persist, project, "frontend-motion").await;
    assert_eq!(findings(&persist, project).await?, 1);
    let detail = persist
        .launch()
        .get_work_thread(project, "frontend-motion", HistoryPage::default())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(detail.thread.status.as_deref(), Some("active"));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn run_with_a_failing_client_writes_nothing() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let project = checkpointed(&persist).await?;
    let server = stub(500, json!({})).await;
    let decider = decider(&server, &persist)?;
    run(&decider, &persist, project, "frontend-motion").await;
    assert_eq!(findings(&persist, project).await?, 0);
    persist.pool().close().await;
    db.cleanup().await
}
