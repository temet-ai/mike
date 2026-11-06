use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::correlation::fetch_actor_correlation;
use crate::api::handlers::SessionSummary;
use crate::config::AppState;
use crate::fleet::correlate::{
    ActorNode, RunEvent, TurnReport, correlate_actors, extract_run_events,
};
use crate::fleet::derive::{derive_role, derive_state};
use crate::fleet::finite_client::{RunSummary, fetch_run_manifest, fetch_runs_list};
use crate::store::persistence::fleet_correlation::ActorEnrichment;

#[derive(Serialize)]
pub(crate) struct TurnReportDto {
    pub(crate) ts: String,
    pub(crate) progress: Value,
    pub(crate) blockers: Value,
    pub(crate) findings: Value,
    pub(crate) artifacts: Value,
}

impl From<TurnReport> for TurnReportDto {
    fn from(report: TurnReport) -> Self {
        Self {
            ts: report.ts,
            progress: report.progress,
            blockers: report.blockers,
            findings: report.findings,
            artifacts: report.artifacts,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct FleetNodeDto {
    pub(crate) actor_id: String,
    pub(crate) parent_node_id: Option<String>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) workspace: Option<String>,
    pub(crate) terminal: bool,
    pub(crate) session: Option<SessionSummary>,
    pub(crate) turn_report: Option<TurnReportDto>,
    pub(crate) role: Option<String>,
    pub(crate) agent_type: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) next_steps: Option<String>,
    pub(crate) files: Vec<String>,
    pub(crate) state: &'static str,
}

#[derive(Serialize)]
pub(crate) struct FleetEdgeDto {
    pub(crate) from: String,
    pub(crate) to: String,
}

#[derive(Serialize)]
pub(crate) struct FleetEventDto {
    pub(crate) event_kind: String,
    pub(crate) ts: String,
    pub(crate) actor_id: String,
    pub(crate) verdict: Option<String>,
    pub(crate) payload: Value,
}

impl From<RunEvent> for FleetEventDto {
    fn from(event: RunEvent) -> Self {
        Self {
            event_kind: event.event_kind,
            ts: event.ts,
            actor_id: event.actor_id,
            verdict: event.verdict,
            payload: event.payload,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct FleetRunDto {
    pub(crate) run_id: String,
    pub(crate) finite_base: String,
    pub(crate) nodes: Vec<FleetNodeDto>,
    pub(crate) edges: Vec<FleetEdgeDto>,
    pub(crate) events: Vec<FleetEventDto>,
}

#[derive(Serialize)]
pub(crate) struct FleetRunSummaryDto {
    #[serde(flatten)]
    pub(crate) run: RunSummary,
    pub(crate) actor_count: usize,
    pub(crate) correlated_count: usize,
    pub(crate) live_count: usize,
}

#[derive(Serialize)]
pub(crate) struct FleetRunsListDto {
    pub(crate) finite_base: String,
    pub(crate) runs: Vec<FleetRunSummaryDto>,
}

pub(super) async fn correlate_all_runs(
    client: &reqwest::Client,
    finite_base: &str,
    run_ids: Vec<String>,
) -> HashMap<String, Vec<ActorNode>> {
    use futures::stream::{self, StreamExt};

    stream::iter(run_ids)
        .map(|run_id| {
            let client = client.clone();
            let finite_base = finite_base.to_string();
            async move {
                let result = fetch_run_manifest(&client, &finite_base, &run_id).await;
                (run_id, result)
            }
        })
        .buffer_unordered(8)
        .filter_map(|(run_id, result)| async move {
            match result {
                Ok(manifest) => Some((run_id, correlate_actors(&manifest))),
                Err(e) => {
                    tracing::warn!(error = %e, run_id = %run_id, "fleet: per-run manifest fetch failed");
                    None
                }
            }
        })
        .collect()
        .await
}

fn summarize_actor_counts(
    actors: Option<&Vec<ActorNode>>,
    captured: &HashSet<Uuid>,
    busy_ids: &HashSet<String>,
) -> (usize, usize, usize) {
    let Some(actors) = actors else {
        return (0, 0, 0);
    };
    let mut correlated_count = 0;
    let mut live_count = 0;
    for actor in actors {
        let Some(uuid_str) = actor.session_uuid.as_deref() else {
            continue;
        };
        if let Ok(uuid) = Uuid::parse_str(uuid_str)
            && captured.contains(&uuid)
        {
            correlated_count += 1;
        }
        if busy_ids.contains(uuid_str) {
            live_count += 1;
        }
    }
    (actors.len(), correlated_count, live_count)
}

pub(crate) async fn get_fleet_runs(
    State(state): State<AppState>,
) -> Result<Json<FleetRunsListDto>, Response> {
    let runs = fetch_runs_list(&state.client, &state.config.finite_url)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                finite_url = %state.config.finite_url,
                "fleet runs list fetch failed"
            );
            super::fleet_error(
                StatusCode::BAD_GATEWAY,
                "finite_unreachable",
                format!("failed to fetch finite run list: {e}"),
            )
        })?;

    let run_ids: Vec<String> = runs.iter().map(|r| r.run_id.clone()).collect();
    let by_run = correlate_all_runs(&state.client, &state.config.finite_url, run_ids).await;

    let all_uuids: Vec<Uuid> = by_run
        .values()
        .flatten()
        .filter_map(|actor| actor.session_uuid.as_deref())
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();
    let captured = match state.persist() {
        Some(handle) => handle
            .fetch_captured_session_uuids(&all_uuids)
            .await
            .unwrap_or_default(),
        None => HashSet::new(),
    };
    let busy_ids = state.in_flight.busy_session_ids();

    let runs = runs
        .into_iter()
        .map(|run| {
            let (actor_count, correlated_count, live_count) =
                summarize_actor_counts(by_run.get(&run.run_id), &captured, &busy_ids);
            FleetRunSummaryDto {
                run,
                actor_count,
                correlated_count,
                live_count,
            }
        })
        .collect();

    Ok(Json(FleetRunsListDto {
        finite_base: state.config.finite_url.clone(),
        runs,
    }))
}

pub(crate) async fn get_fleet_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<FleetRunDto>, Response> {
    let manifest = fetch_run_manifest(&state.client, &state.config.finite_url, &run_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                run_id = %run_id,
                finite_url = %state.config.finite_url,
                "fleet run manifest fetch failed"
            );
            super::fleet_error(
                StatusCode::BAD_GATEWAY,
                "finite_unreachable",
                format!("failed to fetch finite run manifest: {e}"),
            )
        })?;

    let events = extract_run_events(&manifest)
        .into_iter()
        .map(FleetEventDto::from)
        .collect();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for actor in correlate_actors(&manifest) {
        if let Some(parent) = &actor.parent_node_id {
            edges.push(FleetEdgeDto {
                from: parent.clone(),
                to: actor.actor_id.clone(),
            });
        }
        let correlation = match &actor.session_uuid {
            Some(uuid) => fetch_actor_correlation(&state, uuid).await,
            None => None,
        };
        let (session, enrichment, registration_liveness) = match correlation {
            Some((summary, enrichment, liveness)) => (Some(summary), enrichment, liveness),
            None => (None, ActorEnrichment::default(), None),
        };
        let busy = session.as_ref().is_some_and(|s| s.busy);
        let last_status = session.as_ref().map(|s| s.last_status.as_str());
        let last_activity_at = session.as_ref().and_then(|s| {
            DateTime::parse_from_rfc3339(&s.last_seen)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });
        let state_label = derive_state(
            Utc::now(),
            busy,
            last_activity_at,
            registration_liveness,
            last_status,
        );
        let role = derive_role(enrichment.declared_role.as_deref());

        nodes.push(FleetNodeDto {
            actor_id: actor.actor_id,
            parent_node_id: actor.parent_node_id,
            session_uuid: actor.session_uuid,
            model: actor.model,
            workspace: actor.workspace,
            terminal: actor.terminal,
            session,
            turn_report: actor.latest_turn_report.map(TurnReportDto::from),
            role,
            agent_type: enrichment.agent_type,
            goal: enrichment.goal,
            next_steps: enrichment.next_steps,
            files: enrichment.files,
            state: state_label,
        });
    }

    Ok(Json(FleetRunDto {
        run_id: manifest.run_id,
        finite_base: state.config.finite_url.clone(),
        nodes,
        edges,
        events,
    }))
}
