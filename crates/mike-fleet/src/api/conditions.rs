use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::conditions::{
    ConditionError, ConditionFilter, ConditionRow, NewCondition,
};
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::work_threads::WorkThreadError;
use crate::web_base::{persist_error_response, persist_unavailable_response};

const DEFAULT_CHECK_EVERY_SECS: i32 = 300;
const DEFAULT_LIST_LIMIT: i64 = 50;

fn default_action() -> String {
    "return".to_owned()
}

#[derive(Debug, Deserialize)]
pub struct CreateConditionRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub spec: Value,
    #[serde(default)]
    pub not_before: Option<DateTime<Utc>>,
    #[serde(default)]
    pub after_secs: Option<i64>,
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deadline_secs: Option<i64>,
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default)]
    pub target_session: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    pub brief: String,
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub check_every_secs: Option<i32>,
}

pub struct BuildContext {
    pub machine_uuid: Uuid,
    pub project_id: Option<i64>,
    pub now: DateTime<Utc>,
}

fn invalid(message: String) -> ConditionError {
    ConditionError::Invalid(message)
}

fn parse_uuid(label: &str, value: &str) -> Result<Uuid, ConditionError> {
    Uuid::parse_str(value.trim())
        .map_err(|e| invalid(format!("{label} `{value}` is not a UUID: {e}")))
}

fn optional_uuid(label: &str, value: Option<&str>) -> Result<Option<Uuid>, ConditionError> {
    value.map(|v| parse_uuid(label, v)).transpose()
}

fn is_session_kind(kind: &str) -> bool {
    matches!(kind, "session_turn_ended" | "session_ended")
}

fn spec_object(kind: &str, spec: Value, session: Option<Uuid>) -> Result<Value, ConditionError> {
    let mut map = match spec {
        Value::Null => Map::new(),
        Value::Object(map) => map,
        other => return Err(invalid(format!("spec must be a JSON object, got {other}"))),
    };
    if is_session_kind(kind)
        && !map.contains_key("session_uuid")
        && let Some(session) = session
    {
        map.insert("session_uuid".to_owned(), json!(session));
    }
    Ok(Value::Object(map))
}

fn after(now: DateTime<Utc>, label: &str, secs: i64) -> Result<DateTime<Utc>, ConditionError> {
    if secs < 0 {
        return Err(invalid(format!(
            "{label} is {secs}; it must not be negative"
        )));
    }
    Ok(now + Duration::seconds(secs))
}

fn start_of(
    req: &CreateConditionRequest,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, ConditionError> {
    match (req.not_before, req.after_secs) {
        (Some(at), _) => Ok(at),
        (None, Some(secs)) => after(now, "after_secs", secs),
        (None, None) if req.kind == "time" => Err(invalid(
            "a `time` condition needs not_before or after_secs".to_owned(),
        )),
        (None, None) => Ok(now),
    }
}

fn deadline_of(
    req: &CreateConditionRequest,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, ConditionError> {
    match (req.deadline, req.deadline_secs) {
        (Some(at), _) => Ok(Some(at)),
        (None, Some(secs)) => after(now, "deadline_secs", secs).map(Some),
        (None, None) => Ok(None),
    }
}

pub fn build_new(
    req: CreateConditionRequest,
    ctx: &BuildContext,
) -> Result<NewCondition, ConditionError> {
    let session = optional_uuid("session_id", req.session_id.as_deref())?;
    let target_session = optional_uuid("target_session", req.target_session.as_deref())?;
    let thread_slug = req
        .thread
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if thread_slug.is_some() && ctx.project_id.is_none() {
        return Err(invalid(
            "thread needs a project: pass `project` or the registering `session_id`".to_owned(),
        ));
    }
    let not_before = start_of(&req, ctx.now)?;
    let deadline = deadline_of(&req, ctx.now)?;
    Ok(NewCondition {
        machine_uuid: ctx.machine_uuid,
        created_by_session: session,
        project_id: ctx.project_id,
        thread_slug: thread_slug.map(str::to_owned),
        spec: spec_object(&req.kind, req.spec, session)?,
        kind: req.kind,
        not_before,
        deadline,
        check_every_secs: req.check_every_secs.unwrap_or(DEFAULT_CHECK_EVERY_SECS),
        action: req.action,
        target_session,
        folder: req.folder,
        brief: req.brief,
    })
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

fn error_response(status: StatusCode, error: &'static str, message: String) -> Response {
    (status, Json(ErrorBody { error, message })).into_response()
}

fn condition_error_response(error: ConditionError) -> Response {
    match error {
        ConditionError::Invalid(message) => {
            error_response(StatusCode::BAD_REQUEST, "invalid_condition", message)
        }
        ConditionError::Persist(error) => persist_error_response(error),
    }
}

async fn resolve_project(
    persist: &PersistHandle,
    req: &CreateConditionRequest,
) -> Result<Option<i64>, ConditionError> {
    if let Some(selector) = req
        .project
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return match persist.launch().resolve_work_thread_project(selector).await {
            Ok(Some(project)) => Ok(Some(project.id)),
            Ok(None) => Err(invalid(format!(
                "no project matches `{selector}`; list_projects shows the known ones"
            ))),
            Err(WorkThreadError::Invalid(message) | WorkThreadError::NotFound(message)) => {
                Err(invalid(message))
            }
            Err(WorkThreadError::Persist(error)) => Err(ConditionError::Persist(error)),
        };
    }
    let Some(session) = optional_uuid("session_id", req.session_id.as_deref())? else {
        return Ok(None);
    };
    let presence = persist.fleet().condition_session_presence(session).await?;
    Ok(presence.map(|p| p.project_id))
}

pub async fn post_condition(
    State(state): State<BaseState>,
    Json(req): Json<CreateConditionRequest>,
) -> Response {
    let Some(persist) = state.persist() else {
        return persist_unavailable_response();
    };
    let Some(machine_uuid) = state.resolve_host_machine_uuid().await else {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_machine_identity",
            "this mike has no machine identity, so no worker could fire the condition".to_owned(),
        );
    };
    let project_id = match resolve_project(persist, &req).await {
        Ok(project_id) => project_id,
        Err(error) => return condition_error_response(error),
    };
    let ctx = BuildContext {
        machine_uuid,
        project_id,
        now: Utc::now(),
    };
    let new = match build_new(req, &ctx) {
        Ok(new) => new,
        Err(error) => return condition_error_response(error),
    };
    match persist.fleet().insert_condition(&new).await {
        Ok(row) => (StatusCode::CREATED, Json(row)).into_response(),
        Err(error) => condition_error_response(error),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ListConditionsQuery {
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_conditions(
    State(state): State<BaseState>,
    Query(query): Query<ListConditionsQuery>,
) -> Result<Json<Vec<ConditionRow>>, Response> {
    let Some(persist) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let filter = ConditionFilter {
        thread_slug: query.thread,
        state: query.state,
        created_by_session: optional_uuid("session", query.session.as_deref())
            .map_err(condition_error_response)?,
        limit: query.limit.unwrap_or(DEFAULT_LIST_LIMIT),
    };
    persist
        .fleet()
        .list_conditions(&filter)
        .await
        .map(Json)
        .map_err(condition_error_response)
}

pub async fn cancel_condition(
    State(state): State<BaseState>,
    Path(condition): Path<Uuid>,
) -> Response {
    let Some(persist) = state.persist() else {
        return persist_unavailable_response();
    };
    match persist.fleet().cancel_condition(condition).await {
        Ok(true) => {
            Json(json!({"condition_uuid": condition, "state": "cancelled"})).into_response()
        }
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "condition_not_armed",
            format!(
                "no armed condition has uuid {condition}; list_conditions shows each one's state"
            ),
        ),
        Err(error) => condition_error_response(error),
    }
}

#[cfg(test)]
#[path = "conditions_tests.rs"]
mod conditions_tests;
