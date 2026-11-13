use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::types::SortOrder;
use crate::web_base::{persist_error_response, persist_unavailable_response};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PinRequest {
    pub pinned: bool,
}

#[derive(Deserialize)]
pub struct ProjectNameRequest {
    pub custom_name: Option<String>,
}

#[derive(Deserialize)]
pub struct TtlRequest {
    pub ttl_secs: Option<u64>,
}

pub fn bad_ttl_zero_response() -> Response {
    (
        StatusCode::BAD_REQUEST,
        "ttl_secs must be >= 1; use pinning for indefinite retention",
    )
        .into_response()
}

pub fn parse_ttl_secs(ttl_secs: Option<u64>) -> Option<std::time::Duration> {
    ttl_secs.map(std::time::Duration::from_secs)
}

pub async fn patch_session_pin(
    State(state): State<BaseState>,
    Path(session_id): Path<String>,
    Json(body): Json<PinRequest>,
) -> Response {
    if uuid::Uuid::parse_str(&session_id).is_err() {
        return (StatusCode::NOT_FOUND, "unknown session").into_response();
    }
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    let db_result = if body.pinned {
        persist.launch().pin_session(&session_id).await
    } else {
        match persist.resolve_session_db_id(&session_id).await {
            Ok(Some(db_id)) => persist.launch().unpin_session(db_id).await,
            Ok(None) => Ok(()),
            Err(e) => Err(e),
        }
    };
    if let Err(e) = db_result {
        return persist_error_response(e);
    }
    {
        let mut store = state.store.write().await;
        store.set_session_pinned(&session_id, body.pinned);
    }
    Json(serde_json::json!({
        "session_id": session_id,
        "pinned": body.pinned,
    }))
    .into_response()
}

pub async fn patch_session_ttl(
    State(state): State<BaseState>,
    Path(session_id): Path<String>,
    Json(body): Json<TtlRequest>,
) -> Response {
    if uuid::Uuid::parse_str(&session_id).is_err() {
        return (StatusCode::NOT_FOUND, "unknown session").into_response();
    }
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    if matches!(body.ttl_secs, Some(0)) {
        return bad_ttl_zero_response();
    }
    let ttl = parse_ttl_secs(body.ttl_secs);
    if let Err(e) = persist.launch().set_session_ttl(&session_id, ttl).await {
        return persist_error_response(e);
    }
    let ttl_str = ttl.map(|d| humantime::format_duration(d).to_string());
    {
        let mut store = state.store.write().await;
        store.set_session_ttl_override(&session_id, ttl_str.clone());
    }
    let default_ttl = std::time::Duration::from_secs(state.config.default_ttl_secs);
    let session_db_id = match persist.resolve_session_db_id(&session_id).await {
        Ok(Some(id)) => id,
        Ok(None) => 0i64,
        Err(e) => return persist_error_response(e),
    };
    let effective = match persist
        .launch()
        .effective_ttl_for_session(session_db_id, default_ttl)
        .await
    {
        Ok(t) => t,
        Err(e) => return persist_error_response(e),
    };
    Json(serde_json::json!({
        "session_id": session_id,
        "ttl_secs": body.ttl_secs,
        "effective_ttl_secs": effective.as_secs(),
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct PatchNarrativeOverviewRequest {
    pub overview: String,
}

#[derive(Serialize)]
pub struct NarrativeOverviewResponse {
    pub session_id: uuid::Uuid,
    pub overview: String,
    pub generated_at: Option<String>,
}

pub async fn patch_session_narrative_overview(
    State(state): State<BaseState>,
    Path(session_id): Path<String>,
    Json(body): Json<PatchNarrativeOverviewRequest>,
) -> Result<Json<NarrativeOverviewResponse>, Response> {
    let Ok(session_uuid) = uuid::Uuid::parse_str(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let overview = body.overview.trim();
    if overview.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "overview must not be empty").into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let updated = handle
        .set_session_overview(session_uuid, overview, None)
        .await
        .map_err(persist_error_response)?;
    if !updated {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    Ok(Json(NarrativeOverviewResponse {
        session_id: session_uuid,
        overview: overview.to_string(),
        generated_at: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct NarrativeOverviewLlmOutput {
    pub overview: String,
}

pub const NARRATIVE_OVERVIEW_SYSTEM_PROMPT: &str = "You write a narrative overview of a coding \
    agent session for a human reviewer, from the session digest supplied in the user message. \
    Respond with JSON matching the schema: {\"overview\": \"<text>\"}. The overview MUST be \
    EXACTLY two paragraphs, separated by a single blank line, with no heading, title, or \
    bullet points — plain prose only. The FIRST paragraph states what the session set out to \
    do (the operator's original request or goal). The SECOND paragraph states where the \
    session landed: what was accomplished, what was left incomplete, and the last status.";

pub async fn build_narrative_overview_digest(
    handle: &crate::store::persistence::PersistHandle,
    session_db_id: i64,
    session_id: &str,
) -> Result<String, crate::store::persistence::PersistError> {
    const MAX_TURN_PROMPT_CHARS: usize = 400;

    let summary = handle.fetch_session_summary(session_db_id).await?;
    let (turns, _truncated) = handle
        .list_for_session_bounded(
            session_db_id,
            SortOrder::Asc,
            &crate::store::persistence::operator_turns::SessionOverviewFilter::default(),
        )
        .await?;

    let mut digest = format!("Session: {session_id}\n");
    match &summary {
        Some(s) => {
            digest.push_str(&format!(
                "Project: {}\nSpan: {} to {}\nLast status: {}\n",
                s.project_path,
                s.first_seen.to_rfc3339(),
                s.last_seen.to_rfc3339(),
                s.last_status.as_deref().unwrap_or("unknown"),
            ));
            if let Some(prompt) = &s.originating_operator_prompt {
                digest.push_str(&format!("Originating request: {prompt}\n"));
            }
        }
        None => digest.push_str("(no session summary row yet)\n"),
    }

    let mut files: Vec<&str> = Vec::new();
    for t in &turns {
        for f in &t.files_touched {
            if !files.contains(&f.as_str()) {
                files.push(f);
            }
        }
    }
    if files.is_empty() {
        digest.push_str("Files touched: (none recorded)\n");
    } else {
        digest.push_str(&format!("Files touched: {}\n", files.join(", ")));
    }

    digest.push_str(&format!("\n{} turn(s):\n", turns.len()));
    for (i, t) in turns.iter().enumerate() {
        digest.push_str(&format!(
            "{}. [{}] {}\n",
            i + 1,
            t.outcome,
            crate::analysis::rerank::service::truncate_chars(&t.user_prompt, MAX_TURN_PROMPT_CHARS),
        ));
    }

    Ok(digest)
}

pub async fn patch_project_pin(
    State(state): State<BaseState>,
    Path(project_path): Path<String>,
    Json(body): Json<PinRequest>,
) -> Response {
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    let db_result = if body.pinned {
        persist.launch().pin_project(&project_path).await
    } else {
        persist.launch().unpin_project(&project_path).await
    };
    if let Err(e) = db_result {
        return persist_error_response(e);
    }
    Json(serde_json::json!({
        "project_path": project_path,
        "pinned": body.pinned,
    }))
    .into_response()
}

pub async fn patch_project_name(
    State(state): State<BaseState>,
    Path(project_path): Path<String>,
    Json(body): Json<ProjectNameRequest>,
) -> Response {
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    let normalized = body
        .custom_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match persist
        .launch()
        .set_project_custom_name(&project_path, normalized)
        .await
    {
        Ok(true) => Json(serde_json::json!({
            "project_path": project_path,
            "custom_name": normalized,
        }))
        .into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "unknown project").into_response(),
        Err(e) => persist_error_response(e),
    }
}

pub async fn patch_project_ttl(
    State(state): State<BaseState>,
    Path(project_path): Path<String>,
    Json(body): Json<TtlRequest>,
) -> Response {
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    if matches!(body.ttl_secs, Some(0)) {
        return bad_ttl_zero_response();
    }
    let ttl = parse_ttl_secs(body.ttl_secs);
    if let Err(e) = persist.launch().set_project_ttl(&project_path, ttl).await {
        return persist_error_response(e);
    }
    Json(serde_json::json!({
        "project_path": project_path,
        "ttl_secs": body.ttl_secs,
    }))
    .into_response()
}
