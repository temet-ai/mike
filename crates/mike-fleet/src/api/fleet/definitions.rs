use crate::store::persistence::fleet_repo::FleetPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::at_ready;
use super::definition_patch::{
    Move, PatchFleetRequest, STATE_DRAFT, STATE_READY, STATE_STARTING, next_move, parse_new,
    save_definition,
};
use super::definition_view::{DefinitionError, FleetView, digest_view, seat_view};
use crate::base_state::BaseState;
use crate::launch::fleet::shutdown::{self, DONE, RUNNING, STOPPING, StopReport};
use crate::launch::fleet::tell_digest::{SlotQueue, slot_queue};
use crate::launch::fleet::{
    FleetDefinition, check_definition, definition::task_sessions, start::begin, status::assemble,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetRow, NewFleet};

pub const DEFAULT_LIST_LIMIT: i64 = 50;
pub const MAX_LIST_LIMIT: i64 = 200;
const MAX_SLUG_LENGTH: usize = 64;

#[derive(Debug, Deserialize)]
pub struct CreateFleetRequest {
    slug: String,
    project_path: String,
    #[serde(default)]
    definition: Option<Value>,
    #[serde(default)]
    created_by: Option<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListFleetsQuery {
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub fn slug_problem(slug: &str) -> Option<String> {
    if slug.is_empty() || slug.len() > MAX_SLUG_LENGTH {
        return Some(format!(
            "`slug` must be 1 to {MAX_SLUG_LENGTH} characters; got {} characters",
            slug.len()
        ));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    if !slug.chars().all(allowed) {
        return Some(format!(
            "`slug` may hold only letters, digits, `-`, `_` and `.`; got `{slug}`"
        ));
    }
    if Uuid::parse_str(slug).is_ok() {
        return Some(format!(
            "`slug` must not look like a fleet id, which is how a fleet is told apart from its slug; got `{slug}`"
        ));
    }
    None
}

pub fn persistence(state: &BaseState) -> Result<&PersistHandle, DefinitionError> {
    state.persist().ok_or(DefinitionError::Unavailable)
}

pub async fn load(persist: &PersistHandle, key: &str) -> Result<FleetRow, DefinitionError> {
    persist
        .fleet()
        .find_fleet(key)
        .await?
        .ok_or_else(|| DefinitionError::NotFound(key.to_string()))
}

async fn launch(
    state: &BaseState,
    persist: &PersistHandle,
    row: &FleetRow,
) -> Result<(), DefinitionError> {
    if row.state != STATE_READY {
        return Err(DefinitionError::NotReady {
            slug: row.slug.clone(),
            state: row.state.clone(),
        });
    }
    let started = persist
        .fleet()
        .transition_fleet_state(row, STATE_STARTING, None, None)
        .await?;
    if started.is_none() {
        return Err(DefinitionError::Changed(row.slug.clone()));
    }
    begin(state.clone(), row.fleet_uuid);
    Ok(())
}

pub async fn create_fleet(
    State(state): State<BaseState>,
    Json(body): Json<CreateFleetRequest>,
) -> Result<(StatusCode, Json<FleetView>), DefinitionError> {
    if let Some(problem) = slug_problem(&body.slug) {
        return Err(DefinitionError::InvalidRequest(problem));
    }
    if body.project_path.trim().is_empty() {
        return Err(DefinitionError::InvalidRequest(
            "`project_path` must not be empty".to_string(),
        ));
    }
    let mut requested = body.definition.unwrap_or_else(|| json!({}));
    crate::launch::fleet::operation_profile::apply(&body.project_path, &mut requested)
        .await
        .map_err(DefinitionError::InvalidDefinition)?;
    let (definition, parsed) = parse_new(Some(requested))?;
    let persist = persistence(&state)?;
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: body.slug,
            project_path: body.project_path,
            definition,
            missing: json!(check_definition(&parsed)),
            created_by: body.created_by,
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(at_ready::view(&state, &row).await),
    ))
}

#[derive(Debug, Deserialize, Default)]
pub struct GetFleetQuery {
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    session: Option<String>,
}

fn header(row: &FleetRow, body: Value) -> Value {
    let mut view = json!({
        "fleet": row.fleet_uuid,
        "slug": row.slug,
        "state": row.state,
        "run_id": row.run_id,
    });
    if let (Some(fields), Value::Object(extra)) = (view.as_object_mut(), body) {
        fields.extend(extra);
    }
    view
}

pub async fn get_fleet(
    State(state): State<BaseState>,
    Path(key): Path<String>,
    Query(query): Query<GetFleetQuery>,
) -> Result<Json<Value>, DefinitionError> {
    let row = load(persistence(&state)?, &key).await?;
    match query.view.as_deref() {
        None | Some("full") => {
            let view = at_ready::view(&state, &row).await;
            super::observations::with_observations(
                persistence(&state)?,
                row.run_id.as_deref(),
                &view,
            )
            .await
            .map(Json)
        }
        Some("seat") => {
            let def = FleetDefinition::from_value(&row.definition)
                .map_err(DefinitionError::InvalidDefinition)?;
            let sessions = match (persistence(&state)?, row.run_id.as_deref()) {
                (persist, Some(run)) => task_sessions(persist, run, &def).await,
                _ => Default::default(),
            };
            let body = seat_view(&def, query.session.as_deref(), &sessions);
            Ok(Json(header(&row, body)))
        }
        Some("digest") => {
            let status = assemble(&state, &row)
                .await
                .map_err(DefinitionError::Status)?;
            let queue = match (
                state.persist(),
                row.run_id.as_deref(),
                FleetDefinition::from_value(&row.definition),
            ) {
                (Some(persist), Some(run), Ok(def)) => slot_queue(persist, &def, run).await,
                _ => SlotQueue::default(),
            };
            Ok(Json(header(&row, digest_view(&status, queue))))
        }
        Some(other) => Err(DefinitionError::InvalidRequest(format!(
            "`view` must be `seat`, `digest` or `full`; got `{other}`"
        ))),
    }
}

pub async fn patch_fleet(
    State(state): State<BaseState>,
    Path(key): Path<String>,
    Json(body): Json<PatchFleetRequest>,
) -> Result<Json<FleetView>, DefinitionError> {
    let persist = persistence(&state)?;
    let row = load(persist, &key).await?;
    if row.state == STATE_STARTING {
        return Err(DefinitionError::Starting(row.slug));
    }
    let fleet = row.fleet_uuid;
    let saved = save_definition(&state, persist, row, &body).await?;
    match next_move(&saved.row.state, body.ready, &saved.missing) {
        Move::Stay => {}
        Move::BackToDraft => {
            persist
                .fleet()
                .transition_fleet_state(&saved.row, STATE_DRAFT, None, None)
                .await?
                .ok_or_else(|| DefinitionError::Changed(saved.row.slug.clone()))?;
        }
        Move::MarkReady => {
            let ready = persist
                .fleet()
                .transition_fleet_state(&saved.row, STATE_READY, None, None)
                .await?
                .ok_or_else(|| DefinitionError::Changed(saved.row.slug.clone()))?;
            if state.feature_toggle_overlay.load().fleet_auto_start {
                launch(&state, persist, &ready).await?;
            }
        }
    }
    let fresh = load(persist, &fleet.to_string()).await?;
    Ok(Json(at_ready::view(&state, &fresh).await))
}

pub async fn start_fleet(
    State(state): State<BaseState>,
    Path(key): Path<String>,
) -> Result<Json<FleetView>, DefinitionError> {
    let persist = persistence(&state)?;
    let row = load(persist, &key).await?;
    if row.state != STATE_READY {
        return Err(DefinitionError::NotReady {
            slug: row.slug,
            state: row.state,
        });
    }
    launch(&state, persist, &row).await?;
    let fresh = load(persist, &row.fleet_uuid.to_string()).await?;
    Ok(Json(FleetView::full(&fresh)))
}

pub async fn stop_fleet(
    State(state): State<BaseState>,
    Path(key): Path<String>,
) -> Result<Json<StopReport>, DefinitionError> {
    let persist = persistence(&state)?;
    let row = load(persist, &key).await?;
    if !(matches!(
        row.state.as_str(),
        STATE_STARTING | RUNNING | STOPPING | DONE
    ) || row.state == "failed" && row.run_id.is_some())
    {
        return Err(DefinitionError::NotRunning {
            slug: row.slug,
            state: row.state,
        });
    }
    Ok(Json(shutdown::stop(&state, persist, &row).await?))
}
