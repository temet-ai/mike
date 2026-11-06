use std::sync::Arc;

use axum::response::Response;

use super::roster::{RosterEntryDto, RosterResponseDto, roster_entry_from_row};
use crate::api::handlers::persist_error_response;
use crate::api::poll_answers::{Loaded, PollError, Presented, Weigh, json_bytes, live_hash};
use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::WorkspaceRosterRow;
use crate::workspace::RosterScope;

const ROW_WEIGHT: usize = 4096;

#[derive(Clone)]
pub(crate) struct RosterAnswer {
    project_id: i64,
    project_path: String,
    rows: Arc<Vec<WorkspaceRosterRow>>,
}

impl Weigh for RosterAnswer {
    fn weight(&self) -> usize {
        (self.rows.len() + 1) * ROW_WEIGHT
    }
}

pub(super) async fn load_roster(
    handle: &PersistHandle,
    project_path: String,
    scope: RosterScope,
) -> Result<Loaded<RosterAnswer>, Response> {
    let (project_id, project_path) = super::resolve_project_path(handle, project_path).await?;
    let rows = handle
        .fetch_workspace_roster(project_id, scope)
        .await
        .map_err(persist_error_response)?;
    let sessions = rows.iter().map(|r| r.session_uuid.to_string()).collect();
    Ok(Loaded {
        value: RosterAnswer {
            project_id,
            project_path,
            rows: Arc::new(rows),
        },
        sessions,
    })
}

pub(super) fn present_roster(
    state: &AppState,
    answer: &RosterAnswer,
) -> Result<Presented, PollError> {
    let busy_ids = state.in_flight.busy_session_ids();
    let roster: Vec<RosterEntryDto> = answer
        .rows
        .iter()
        .cloned()
        .map(|r| roster_entry_from_row(r, None, &busy_ids))
        .collect();
    let live = live_hash(roster.iter().map(|entry| entry.busy));
    let body = json_bytes(&RosterResponseDto {
        project_id: answer.project_id,
        project_path: answer.project_path.clone(),
        roster,
    })?;
    Ok(Presented {
        body,
        live: Some(live),
    })
}
