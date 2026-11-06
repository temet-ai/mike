use std::collections::BTreeMap;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::at_ready::AtReady;
use crate::launch::fleet::plan::ordered_tasks;
use crate::launch::fleet::status::FleetStatus;
use crate::launch::fleet::tell_digest::{SlotQueue, rows as digest_rows};
use crate::launch::fleet::{FleetDefinition, definition::TaskDefinition};
use crate::store::persistence::PersistError;
use crate::store::persistence::fleet::{FleetHeaderRow, FleetRow};

#[derive(Debug, Serialize)]
pub(crate) struct FleetView {
    fleet: Uuid,
    slug: String,
    project_path: String,
    state: String,
    run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    definition: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_adoption: Option<Value>,
    missing: Value,
    ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    at_ready: Option<AtReady>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    ready_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

impl FleetView {
    pub(super) fn full(row: &FleetRow) -> Self {
        Self::build(&FleetHeaderRow::from(row), Some(row.definition.clone()))
    }

    pub(super) fn listed(row: &FleetHeaderRow) -> Self {
        Self::build(row, None)
    }

    pub(super) fn with_at_ready(mut self, at_ready: AtReady) -> Self {
        self.at_ready = Some(at_ready);
        self
    }

    fn build(row: &FleetHeaderRow, definition: Option<Value>) -> Self {
        let operation_adoption = definition
            .as_ref()
            .and_then(|value| FleetDefinition::from_value(value).ok())
            .map(|def| crate::launch::fleet::operation::adoption(&def));
        Self {
            fleet: row.fleet_uuid,
            slug: row.slug.clone(),
            project_path: row.project_path.clone(),
            state: row.state.clone(),
            run_id: row.run_id.clone(),
            definition,
            operation_adoption,
            missing: row.missing.clone(),
            ready: row.missing.as_array().is_some_and(Vec::is_empty),
            at_ready: None,
            created_at: row.created_at,
            updated_at: row.updated_at,
            ready_at: row.ready_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        }
    }
}

fn peer_row(id: &str, task: &TaskDefinition, sessions: &BTreeMap<String, Uuid>) -> Value {
    let mut row = Map::new();
    row.insert("task".into(), json!(id));
    row.insert("name".into(), json!(task.name));
    if let Some(session) = sessions.get(id) {
        row.insert("session".into(), json!(session));
    }
    Value::Object(row)
}

fn own_row(id: &str, task: &TaskDefinition, sessions: &BTreeMap<String, Uuid>) -> Value {
    let mut row = peer_row(id, task, sessions);
    if let Value::Object(fields) = &mut row {
        fields.insert("discipline".into(), json!(task.discipline));
        fields.insert("criteria".into(), json!(task.criteria));
        fields.insert("files".into(), json!(task.files));
        if !task.new_files.is_empty() {
            fields.insert("new_files".into(), json!(task.new_files));
        }
        if !task.after.is_empty() {
            fields.insert("after".into(), json!(task.after));
        }
        if !task.required_paths.is_empty() {
            fields.insert("required_paths".into(), json!(task.required_paths));
        }
    }
    row
}

pub(crate) fn seat_view(
    def: &FleetDefinition,
    session: Option<&str>,
    sessions: &BTreeMap<String, Uuid>,
) -> Value {
    let wanted = session.map(str::trim).filter(|s| !s.is_empty());
    let tasks = ordered_tasks(def);
    let own = wanted.and_then(|wanted| {
        tasks
            .iter()
            .find(|(id, _)| {
                sessions
                    .get(id.as_str())
                    .is_some_and(|s| s.to_string().eq_ignore_ascii_case(wanted))
            })
            .map(|(id, _)| id.as_str())
    });
    let peers: Vec<Value> = tasks
        .iter()
        .filter(|(id, _)| Some(id.as_str()) != own)
        .map(|(id, task)| peer_row(id, task, sessions))
        .collect();
    let you = own.and_then(|own| def.tasks.get(own).map(|task| own_row(own, task, sessions)));
    let mut view = json!({ "you": you, "peers": peers });
    view["operation_adoption"] = crate::launch::fleet::operation::adoption(def);
    if !def.shared_file_producers.is_empty() {
        view["shared_file_producers"] = json!(def.shared_file_producers);
    }
    view
}

pub(crate) fn digest_view(status: &FleetStatus, queue: SlotQueue) -> Value {
    let tasks: Vec<Value> = digest_rows(status)
        .into_iter()
        .map(|task| {
            let mut row = Map::new();
            row.insert("task".into(), json!(task.task));
            if let Some(seat) = &task.seat {
                row.insert("seat".into(), json!(seat));
            }
            row.insert("state".into(), json!(task.state));
            row.insert("ready".into(), json!(task.ready));
            row.insert("accepted".into(), json!(task.accepted));
            if let Some(idle) = task.idle_seconds {
                row.insert("idle_seconds".into(), json!(idle));
            }
            row.insert("passes".into(), json!(task.passes));
            row.insert("fails".into(), json!(task.fails));
            Value::Object(row)
        })
        .collect();
    let mut view = json!({ "tasks": tasks, "every_seat_idle_seconds": status.every_seat_idle_seconds,
            "accepted": status.accepted, "criteria": status.criteria, "obligations": status.obligations });
    if queue.waiting > 0 {
        view["waiting_for_slot"] = json!(queue.waiting);
        view["build_slots"] = json!(queue.slots);
    }
    view
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DefinitionError {
    #[error("no fleet is recorded for `{0}`")]
    NotFound(String),
    #[error(
        "the fleet `{0}` is starting, and its definition cannot change while mike launches its seats"
    )]
    Starting(String),
    #[error(
        "the fleet `{0}` is stopping, and its definition cannot change until shutdown finishes"
    )]
    Stopping(String),
    #[error("{0}")]
    InvalidDefinition(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error(
        "the fleet `{0}` was changed by another writer twice while this change was being saved"
    )]
    Changed(String),
    #[error("the fleet `{slug}` is in state `{state}`, and only a ready fleet can be started")]
    NotReady { slug: String, state: String },
    #[error(
        "the fleet `{slug}` is in state `{state}`, and this operation requires a running fleet"
    )]
    NotRunning { slug: String, state: String },
    #[error("the database is not reachable, so fleets cannot be read or written")]
    Unavailable,
    #[error("the fleet could not be read or written: {0}")]
    Persistence(#[from] PersistError),
    #[error("the status of the fleet could not be assembled: {0}")]
    Status(String),
}

impl DefinitionError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "fleet_not_found"),
            Self::Starting(_) => (StatusCode::CONFLICT, "fleet_starting"),
            Self::Stopping(_) => (StatusCode::CONFLICT, "fleet_stopping"),
            Self::InvalidDefinition(_) => (StatusCode::BAD_REQUEST, "invalid_definition"),
            Self::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Changed(_) => (StatusCode::CONFLICT, "fleet_changed"),
            Self::NotReady { .. } => (StatusCode::CONFLICT, "fleet_not_ready"),
            Self::NotRunning { .. } => (StatusCode::CONFLICT, "fleet_not_running"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "persistence_unavailable"),
            Self::Persistence(_) => (StatusCode::INTERNAL_SERVER_ERROR, "persistence_failed"),
            Self::Status(_) => (StatusCode::INTERNAL_SERVER_ERROR, "status_failed"),
        }
    }

    fn remedy(&self) -> Option<&'static str> {
        match self {
            Self::NotFound(_) => {
                Some("Check the fleet id or slug, or create the fleet first with define_fleet.")
            }
            Self::Starting(_) => {
                Some("Wait for the start to finish, then read the fleet again with get_fleet.")
            }
            Self::InvalidDefinition(_) => {
                Some("Nothing was saved. Fix the item named above and send the patch again.")
            }
            Self::Changed(_) => Some(
                "Nothing was saved. Read the fleet again with get_fleet and send your patch again.",
            ),
            Self::NotReady { .. } => Some(
                "Fill in what `missing` lists and ask for ready again; a fleet becomes ready \
                 only when every check passes.",
            ),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct DefinitionErrorBody {
    error: &'static str,
    message: String,
    remedy: Option<&'static str>,
}

impl IntoResponse for DefinitionError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if status.is_server_error() {
            tracing::warn!(error = %self, code, "fleet definition request failed");
        }
        let body = DefinitionErrorBody {
            error: code,
            message: self.to_string(),
            remedy: self.remedy(),
        };
        (status, Json(body)).into_response()
    }
}
