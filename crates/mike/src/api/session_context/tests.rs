use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use super::{ContextRequest, post_session_context};
use crate::config::{AppState, ProxyConfig};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{acquire_ephemeral_db, starve_pool};

fn request() -> Json<ContextRequest> {
    Json(ContextRequest {
        project: "/tmp/session-context-test".to_owned(),
        serves: Vec::new(),
        prompt: None,
        cwd: None,
    })
}

#[tokio::test]
async fn context_non_uuid_session_is_400() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let answer = post_session_context(State(state), Path("not-a-uuid".to_owned()), request()).await;
    assert_eq!(answer.err(), Some(StatusCode::BAD_REQUEST));
    Ok(())
}

#[tokio::test]
async fn context_answers_the_identity_block_tagged_with_its_stage() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.self_identity_reminder = true;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    let id = Uuid::new_v4().to_string();
    let Ok(Json(answer)) =
        post_session_context(State(state.clone()), Path(id.clone()), request()).await
    else {
        anyhow::bail!("a UUID session must be answered");
    };
    let [block] = answer.blocks.as_slice() else {
        anyhow::bail!("expected one block: {answer:?}");
    };
    assert_eq!(block.stage, "self_identity");
    assert!(block.text.contains(&id));
    assert!(state.self_identity_served.contains(&id));
    Ok(())
}

#[tokio::test]
async fn context_while_the_pool_is_stalled_answers_inside_the_prompt_budget() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.self_identity_reminder = true;
    toggles.workspace_briefing = true;
    toggles.change_record_tail = true;
    toggles.reminders = true;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    state.persist = Some(handle.clone());
    let id = Uuid::new_v4().to_string();
    let stall = starve_pool(handle.pool(), Duration::from_secs(3)).await?;
    let started = Instant::now();
    let answer = post_session_context(State(state.clone()), Path(id.clone()), request()).await;
    let elapsed = started.elapsed();
    eprintln!("context during a 3 s pool stall answered in {elapsed:?}");
    assert!(answer.is_ok());
    assert!(elapsed < Duration::from_millis(300), "{elapsed:?}");
    assert!(!state.change_record_tail_served.contains(&id));
    assert!(!state.workspace_briefing_served.contains(&id));
    stall.await?;
    state.shutdown().cancel();
    db.cleanup().await
}
