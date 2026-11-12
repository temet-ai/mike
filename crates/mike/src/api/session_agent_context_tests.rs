use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use super::{AgentContextRequest, post_session_agent_context};
use crate::config::ProxyConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::state::AppState;

fn agent_snapshot() -> LessonSnapshot {
    LessonSnapshot::from_agent_fixtures(
        [("rust-developer".to_owned(), 11_i64)],
        [(
            11_i64,
            vec![
                ("correction".to_owned(), "run the narrow check".to_owned()),
                (
                    "convention".to_owned(),
                    "format only your own files".to_owned(),
                ),
            ],
        )],
    )
}

fn request(agent_type: &str) -> Json<AgentContextRequest> {
    Json(AgentContextRequest {
        agent_type: agent_type.to_owned(),
    })
}

#[tokio::test]
async fn agent_context_non_uuid_session_is_400() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    let answer = post_session_agent_context(
        State(state),
        Path("not-a-uuid".to_owned()),
        request("rust-developer"),
    )
    .await;
    assert_eq!(answer.err(), Some(StatusCode::BAD_REQUEST));
    Ok(())
}

#[tokio::test]
async fn agent_context_unknown_agent_answers_null() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    state.lesson_cache.store(Arc::new(agent_snapshot()));
    let id = Uuid::new_v4().to_string();
    let Ok(Json(answer)) =
        post_session_agent_context(State(state.clone()), Path(id.clone()), request("nobody")).await
    else {
        anyhow::bail!("a UUID session must be answered");
    };
    assert_eq!(answer.agent_id, None);
    assert!(answer.lesson_ids.is_empty());
    assert_eq!(answer.text, None);
    assert!(
        !state
            .lesson_cache
            .load()
            .agent_lesson_served_by_client(&id, "nobody")
    );
    Ok(())
}

#[tokio::test]
async fn agent_context_resolves_agent_type_by_name_and_answers_stage_text() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::feature_toggles::serve_every_stage_for_tests(&state);
    state.lesson_cache.store(Arc::new(agent_snapshot()));
    let id = Uuid::new_v4().to_string();
    let Ok(Json(answer)) = post_session_agent_context(
        State(state.clone()),
        Path(id.clone()),
        request("rust-developer"),
    )
    .await
    else {
        anyhow::bail!("a UUID session must be answered");
    };
    assert_eq!(answer.agent_id, Some(11));
    assert_eq!(answer.lesson_ids.len(), 2);
    let Some(text) = answer.text.clone() else {
        anyhow::bail!("expected reminder text for a resolved agent with lessons");
    };
    assert!(text.contains("run the narrow check"));
    assert!(
        state
            .lesson_cache
            .load()
            .agent_lesson_served_by_client(&id, "rust-developer")
    );
    Ok(())
}
