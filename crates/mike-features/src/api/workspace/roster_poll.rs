use crate::store::persistence::proxy_repo::ProxyPersist as _;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use std::sync::Arc;

use axum::response::Response;

use super::roster::{RosterEntryDto, RosterResponseDto, roster_entry_from_row};
use crate::api::poll_answers::{Loaded, PollError, Presented, Weigh, json_bytes, live_hash};
use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::WorkspaceRosterRow;
use crate::web_base::persist_error_response;
use crate::workspace::RosterScope;

const ROW_WEIGHT: usize = 4096;

#[derive(Clone)]
pub struct RosterAnswer {
    project_id: i64,
    project_path: String,
    rows: Arc<Vec<WorkspaceRosterRow>>,
    observations: Arc<
        std::collections::HashMap<
            uuid::Uuid,
            crate::store::persistence::runtime::SessionObservationRow,
        >,
    >,
}

impl Weigh for RosterAnswer {
    fn weight(&self) -> usize {
        (self.rows.len() + 1) * ROW_WEIGHT
    }
}

pub async fn load_roster(
    handle: &PersistHandle,
    project_path: String,
    scope: RosterScope,
) -> Result<Loaded<RosterAnswer>, Response> {
    let (project_id, project_path) = super::resolve_project_path(handle, project_path).await?;
    let rows = handle
        .proxy()
        .fetch_workspace_roster(project_id, scope)
        .await
        .map_err(persist_error_response)?;
    let ids: Vec<_> = rows.iter().map(|row| row.session_uuid).collect();
    let observations = handle
        .runtime()
        .session_observations(&ids)
        .await
        .map_err(persist_error_response)?
        .into_iter()
        .map(|row| (row.session_uuid, row))
        .collect();
    let sessions = rows.iter().map(|r| r.session_uuid.to_string()).collect();
    Ok(Loaded {
        value: RosterAnswer {
            project_id,
            project_path,
            rows: Arc::new(rows),
            observations: Arc::new(observations),
        },
        sessions,
    })
}

pub fn present_roster(state: &BaseState, answer: &RosterAnswer) -> Result<Presented, PollError> {
    let busy_ids = state.in_flight.busy_session_ids();
    let roster: Vec<RosterEntryDto> = answer
        .rows
        .iter()
        .cloned()
        .map(|r| {
            let observation = answer.observations.get(&r.session_uuid).cloned();
            let mut entry = roster_entry_from_row(r, None, &busy_ids);
            entry.busy |= observation
                .as_ref()
                .is_some_and(|row| row.lifecycle.as_deref() == Some("active"));
            entry.observation = observation;
            entry
        })
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
