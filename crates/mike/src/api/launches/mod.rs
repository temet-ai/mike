mod brief;
pub(crate) mod ledger;
pub(crate) mod mcp_seen;

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::handlers::{persist_error_response, persist_unavailable_response};
use crate::config::AppState;
use crate::launch::backend_choice::CallerTerminal;
use crate::launch::{
    BackendId, DoctorReport, KillReport, LaunchError, ServerOpenRequest, ServerOpenStart,
    doctor_for_server, kill_for_server, open_for_server,
};
use crate::store::persistence::launch::{LaunchListFilter, LaunchRow};

pub(crate) use brief::push_launch_brief;

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateLaunchRequest {
    pub(crate) folder: String,
    #[serde(default)]
    pub(crate) claude_args: Vec<String>,
    #[serde(default)]
    pub(crate) prompt: Option<String>,
    #[serde(default)]
    pub(crate) run: Option<String>,
    #[serde(default)]
    pub(crate) seat: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) role: Option<String>,
    #[serde(default)]
    pub(crate) instructions: Option<String>,
    #[serde(default)]
    pub(crate) goal: Option<String>,
    #[serde(default)]
    pub(crate) worktree_ok: bool,
    #[serde(default)]
    pub(crate) trust: bool,
    #[serde(default = "default_true")]
    pub(crate) proxy_env: bool,
    #[serde(default = "default_true")]
    pub(crate) remote_control: bool,
    #[serde(default = "default_true")]
    pub(crate) channels: bool,
    #[serde(default)]
    pub(crate) backend: Option<BackendId>,
    #[serde(default)]
    pub(crate) caller_env: BTreeMap<String, String>,
}

impl From<CreateLaunchRequest> for ServerOpenRequest {
    fn from(body: CreateLaunchRequest) -> Self {
        Self {
            folder: body.folder,
            claude_args: body.claude_args,
            prompt: body.prompt,
            run: body.run,
            seat: body.seat,
            session_name: body.name,
            role: body.role,
            instructions: body.instructions,
            goal: body.goal,
            template_id: None,
            worktree_ok: body.worktree_ok,
            trust: body.trust,
            proxy_env: body.proxy_env,
            remote_control: body.remote_control,
            channels: body.channels,
            backend: body.backend,
            caller: CallerTerminal::from_env_map(&body.caller_env),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateLaunchResponse {
    pub(crate) launch_id: Option<i64>,
    pub(crate) session: Option<Uuid>,
    pub(crate) backend: BackendId,
    pub(crate) backend_reason: String,
}

impl From<ServerOpenStart> for CreateLaunchResponse {
    fn from(start: ServerOpenStart) -> Self {
        Self {
            launch_id: start.launch_id,
            session: start.session,
            backend: start.backend,
            backend_reason: start.backend_reason,
        }
    }
}

#[derive(Debug, Serialize)]
struct LaunchErrorBody {
    error: &'static str,
    message: String,
    remedy: Option<String>,
}

fn launch_error_status(err: &LaunchError) -> (StatusCode, &'static str) {
    match err {
        LaunchError::WithPaneOutput { source, .. } => launch_error_status(source),
        LaunchError::Usage(_) | LaunchError::Refused(_) => {
            (StatusCode::BAD_REQUEST, "invalid_launch_request")
        }
        LaunchError::Unsupported(_) => (StatusCode::BAD_REQUEST, "unsupported"),
        LaunchError::Unconfirmed(_) => (StatusCode::CONFLICT, "unconfirmed"),
        LaunchError::BackendUnavailable { .. } | LaunchError::BackendNone => {
            (StatusCode::SERVICE_UNAVAILABLE, "backend_unavailable")
        }
        LaunchError::ExecutableNotFound { .. } => {
            (StatusCode::SERVICE_UNAVAILABLE, "executable_not_found")
        }
        LaunchError::Failed(_)
        | LaunchError::Io { .. }
        | LaunchError::Timeout { .. }
        | LaunchError::BackendResponse { .. }
        | LaunchError::Malformed { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "launch_failed"),
    }
}

fn launch_error_response(err: &LaunchError) -> Response {
    let (status, code) = launch_error_status(err);
    (
        status,
        Json(LaunchErrorBody {
            error: code,
            message: err.to_string(),
            remedy: err.remedy().map(str::to_string),
        }),
    )
        .into_response()
}

fn launch_not_found(session: Uuid) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(LaunchErrorBody {
            error: "launch_not_found",
            message: format!("no launch is recorded for session {session}"),
            remedy: None,
        }),
    )
        .into_response()
}

pub(crate) async fn create_launch(
    State(state): State<AppState>,
    Json(body): Json<CreateLaunchRequest>,
) -> Response {
    if body.folder.trim().is_empty() {
        return launch_error_response(&LaunchError::Usage("folder must not be empty".to_string()));
    }
    match open_for_server(state, body.into()).await {
        Ok(start) => (
            StatusCode::ACCEPTED,
            Json(CreateLaunchResponse::from(start)),
        )
            .into_response(),
        Err(e) => launch_error_response(&e),
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListLaunchesQuery {
    #[serde(default)]
    pub(crate) run: Option<String>,
}

pub(crate) async fn list_launches(
    State(state): State<AppState>,
    Query(query): Query<ListLaunchesQuery>,
) -> Result<Json<Vec<LaunchRow>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let filter = LaunchListFilter {
        run_id: query.run,
        open_only: false,
        backend: None,
        limit: LaunchListFilter::DEFAULT_LIMIT,
    };
    handle
        .list_launches(&filter)
        .await
        .map(Json)
        .map_err(persist_error_response)
}

pub(crate) async fn get_launch(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
) -> Result<Json<LaunchRow>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    match handle.find_launch_by_session(session).await {
        Ok(Some(row)) => Ok(Json(row)),
        Ok(None) => Err(launch_not_found(session)),
        Err(e) => Err(persist_error_response(e)),
    }
}

pub(crate) async fn kill_launch(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
) -> Result<Json<KillReport>, Response> {
    kill_for_server(&state, session)
        .await
        .map(Json)
        .map_err(|e| launch_error_response(&e))
}

pub(crate) async fn get_launch_doctor(
    State(state): State<AppState>,
) -> Result<Json<DoctorReport>, Response> {
    doctor_for_server(&state)
        .await
        .map(Json)
        .map_err(|e| launch_error_response(&e))
}
