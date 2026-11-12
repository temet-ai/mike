use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use super::{RecallRequest, post_session_recall};
use crate::config::ProxyConfig;
use crate::recall_injector::{RecallInjectorConfig, answer_recall, is_served};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support;

fn request(project: &str) -> Json<RecallRequest> {
    Json(RecallRequest {
        project: project.to_owned(),
        topics: vec!["deploy".to_owned()],
    })
}

#[tokio::test]
async fn recall_non_uuid_session_is_400_and_not_marked() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    let answer = post_session_recall(
        State(state),
        Path("not-a-uuid".to_owned()),
        request("/tmp/session-recall-test"),
    )
    .await;
    assert_eq!(answer.err(), Some(StatusCode::BAD_REQUEST));
    assert!(!is_served("not-a-uuid"));
    Ok(())
}

#[tokio::test]
async fn recall_with_its_toggle_off_is_409_and_not_marked() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let id = Uuid::new_v4().to_string();
    let answer = post_session_recall(
        State(state),
        Path(id.clone()),
        request("/tmp/session-recall-test"),
    )
    .await;
    assert_eq!(answer.err(), Some(StatusCode::CONFLICT));
    assert!(!is_served(&id));
    Ok(())
}

#[tokio::test]
async fn recall_uuid_session_is_marked_and_answers_null_without_persist() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    let id = Uuid::new_v4().to_string();
    let Ok(Json(answer)) = post_session_recall(
        State(state),
        Path(id.clone()),
        request("/tmp/session-recall-test"),
    )
    .await
    else {
        anyhow::bail!("a UUID session must be answered");
    };
    assert_eq!(answer.topics, vec!["deploy".to_owned()]);
    assert!(answer.lesson_ids.is_empty());
    assert!(answer.text.is_none());
    assert!(is_served(&id));
    Ok(())
}

#[tokio::test]
async fn recall_unknown_project_answers_null_text() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_path = format!("/tmp/recall-unknown-{}", Uuid::new_v4());
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let answer = answer_recall(&handle, &project_path, &["deploy".to_owned()], &cfg).await;
    assert!(answer.text.is_none());
    assert!(answer.lesson_ids.is_empty());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_while_the_pool_is_stalled_answers_in_budget_and_leaves_recall_to_the_proxy()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    state.persist = Some(handle.clone());
    let id = Uuid::new_v4().to_string();
    let stall = test_support::starve_pool(handle.pool(), Duration::from_secs(3)).await?;
    let started = Instant::now();
    let answer = post_session_recall(
        State(state.clone()),
        Path(id.clone()),
        request("/tmp/session-recall-stall"),
    )
    .await;
    let elapsed = started.elapsed();
    eprintln!("recall during a 3 s pool stall answered in {elapsed:?}");
    assert!(answer.is_ok());
    assert!(elapsed < Duration::from_millis(300), "{elapsed:?}");
    assert!(!is_served(&id));
    stall.await?;
    state.shutdown().cancel();
    db.cleanup().await
}
