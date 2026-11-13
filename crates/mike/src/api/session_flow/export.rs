use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use super::dto::SessionFlowComponent;
use super::export_trace::{PerfettoTrace, build_trace};
use super::handler::map_read_error;
use super::map;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_flow::{self as persist, FlowReadError};

const MAX_PAGES_PER_COMPONENT: u32 = 2_000;
const EXPORT_PAGE_LIMIT: u16 = 500;

const ALL_COMPONENTS: [persist::FlowComponent; 14] = [
    persist::FlowComponent::Requests,
    persist::FlowComponent::Dispatches,
    persist::FlowComponent::Tools,
    persist::FlowComponent::Results,
    persist::FlowComponent::Messages,
    persist::FlowComponent::WorkerStatus,
    persist::FlowComponent::ManifestStages,
    persist::FlowComponent::LessonInjections,
    persist::FlowComponent::VerifierEvaluations,
    persist::FlowComponent::Judgments,
    persist::FlowComponent::LlmCalls,
    persist::FlowComponent::Embeddings,
    persist::FlowComponent::Consolidation,
    persist::FlowComponent::Interventions,
];

#[derive(Debug, Deserialize)]
pub(crate) struct SessionFlowExportQuery {
    component: Option<SessionFlowComponent>,
}

pub(super) struct MergedFlow {
    pub(super) session: Uuid,
    pub(super) availability: Option<persist::FlowAvailability>,
    pub(super) actors: Vec<persist::FlowActor>,
    pub(super) activities: Vec<persist::FlowActivity>,
    pub(super) relationships: Vec<persist::FlowRelationship>,
    pub(super) coverage: Vec<persist::FlowCoverage>,
}

impl MergedFlow {
    pub(super) fn new(session: Uuid) -> Self {
        Self {
            session,
            availability: None,
            actors: Vec::new(),
            activities: Vec::new(),
            relationships: Vec::new(),
            coverage: Vec::new(),
        }
    }
}

pub(crate) async fn get_session_flow_export(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SessionFlowExportQuery>,
) -> Result<Json<PerfettoTrace>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let session_id = session.to_string();
    let components: Vec<persist::FlowComponent> = match &query.component {
        Some(component) => vec![map::persist_component(component)],
        None => ALL_COMPONENTS.to_vec(),
    };

    let mut merged = MergedFlow::new(session);
    for component in components {
        let found = collect_component(handle, session, component, &mut merged)
            .await
            .map_err(|e| map_read_error(&session_id, e))?;
        if !found {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    Ok(Json(build_trace(merged)))
}

async fn collect_component(
    handle: &PersistHandle,
    session: Uuid,
    component: persist::FlowComponent,
    merged: &mut MergedFlow,
) -> Result<bool, FlowReadError> {
    let mut cursor = None;
    let mut coverage: Option<persist::FlowCoverage> = None;
    let mut pages = 0u32;
    let mut capped = false;
    loop {
        let Some(page) = handle
            .web()
            .fetch_session_flow(
                session,
                component.clone(),
                EXPORT_PAGE_LIMIT,
                cursor.as_ref(),
            )
            .await?
        else {
            return Ok(false);
        };
        if merged.availability.is_none() {
            merged.availability = Some(page.availability.clone());
        }
        merged.actors.extend(page.actors);
        merged.activities.extend(page.activities);
        merged.relationships.extend(page.relationships);
        coverage = Some(match coverage {
            Some(prev) => merge_coverage(prev, page.coverage),
            None => page.coverage,
        });
        pages += 1;
        let next = page.next_cursor;
        if next.is_none() {
            break;
        }
        if pages >= MAX_PAGES_PER_COMPONENT {
            capped = true;
            break;
        }
        cursor = next;
    }
    if let Some(mut coverage) = coverage {
        if capped {
            coverage.stale_reason = Some(cap_reason(pages, coverage.stale_reason));
            coverage.exhausted = false;
        }
        merged.coverage.push(coverage);
    }
    Ok(true)
}

fn cap_reason(pages: u32, prior: Option<String>) -> String {
    let note = format!(
        "export_page_cap: stopped after {pages} pages ({MAX_PAGES_PER_COMPONENT} page limit); \
         remaining rows stay reachable via GET .../flow"
    );
    match prior {
        Some(existing) if existing != note => format!("{existing}; {note}"),
        _ => note,
    }
}

pub(super) fn merge_coverage(
    a: persist::FlowCoverage,
    b: persist::FlowCoverage,
) -> persist::FlowCoverage {
    persist::FlowCoverage {
        component: b.component,
        lower_id: a.lower_id.or(b.lower_id),
        upper_id: b.upper_id.or(a.upper_id),
        returned_through_id: b.returned_through_id.or(a.returned_through_id),
        exhausted: a.exhausted && b.exhausted,
        retention: merge_retention(a.retention, b.retention),
        capture_complete: merge_capture_complete(a.capture_complete, b.capture_complete),
        latest_only: a.latest_only || b.latest_only,
        best_effort: a.best_effort || b.best_effort,
        omitted_components: union_components(a.omitted_components, b.omitted_components),
        server_observed_at: b.server_observed_at,
        stale_reason: merge_reason(a.stale_reason, b.stale_reason),
    }
}

pub(super) fn merge_retention(
    a: persist::FlowRetention,
    b: persist::FlowRetention,
) -> persist::FlowRetention {
    use persist::FlowRetention::{Partial, Retained, Unknown};
    match (a, b) {
        (Partial, _) | (_, Partial) => Partial,
        (Retained, _) | (_, Retained) => Retained,
        (Unknown, Unknown) => Unknown,
    }
}

pub(super) fn merge_capture_complete(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (None, _) | (_, None) => None,
        (Some(true), Some(true)) => Some(true),
    }
}

pub(super) fn merge_reason(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) if a != b => Some(format!("{a}; {b}")),
        (Some(a), _) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn union_components(
    mut a: Vec<persist::FlowComponent>,
    b: Vec<persist::FlowComponent>,
) -> Vec<persist::FlowComponent> {
    for item in b {
        if !a.contains(&item) {
            a.push(item);
        }
    }
    a
}
