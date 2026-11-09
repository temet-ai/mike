use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::api::client_serves::by_deadline;
use crate::config::AppState;
use crate::launch::seat_fact::{SeatRole, seat_role};
use crate::memory_keeper::deliver::{CardAsk, card_for, serves_cards};
use crate::memory_keeper::supply::Carrier;
use crate::reminders::client_context::{
    ContextBlock, findings_ready_within_budget, prompt_context,
};
use crate::reminders::git_read_hint;

const CARD_BUDGET: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
pub(crate) struct ContextRequest {
    project: String,
    #[serde(default)]
    serves: Vec<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ContextResponse {
    blocks: Vec<ContextBlock>,
    findings_ready: bool,
}

pub(crate) async fn post_session_context(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ContextRequest>,
) -> Result<Json<ContextResponse>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let (mut blocks, findings_ready) = tokio::join!(
        prompt_context(&state, session, &id, &request.project),
        findings_ready_within_budget(&state, &id),
    );
    if request.serves.iter().any(|s| s == "git_read_hint") {
        blocks.extend(git_read_hint::hand_over(&state, &id).await);
    }
    if request.serves.iter().any(|s| s == "memory_keeper") {
        blocks.extend(memory_keeper_block(&state, session, &id, &request).await);
    }
    Ok(Json(ContextResponse {
        blocks,
        findings_ready,
    }))
}

async fn memory_keeper_block(
    state: &AppState,
    session: Uuid,
    id: &str,
    request: &ContextRequest,
) -> Option<ContextBlock> {
    if !serves_cards(state) {
        return None;
    }
    if !matches!(seat_role(state, id).await, SeatRole::Plain | SeatRole::Fork) {
        return None;
    }
    let (state, project, prompt, cwd) = (
        state.clone(),
        request.project.clone(),
        request.prompt.clone(),
        request.cwd.clone(),
    );
    let block = by_deadline(Instant::now() + CARD_BUDGET, async move {
        card_for(
            &state,
            &CardAsk {
                session,
                project_path: &project,
                prompt: prompt.as_deref(),
                cwd: cwd.as_deref(),
                carrier: Carrier::Tether,
            },
        )
        .await
    })
    .await
    .flatten()?;
    Some(ContextBlock {
        stage: "memory_keeper",
        text: block.text,
    })
}

#[cfg(test)]
mod tests;
