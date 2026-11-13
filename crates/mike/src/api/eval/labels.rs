use crate::store::persistence::eval_repo::EvalPersist as _;
use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::eval::labels::SpaOrigin;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_labels::{
    GoldEntry, GoldWrite, LabelQuery, LabelWriteError, Subject, VocabularyRow,
};

pub(crate) fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

pub(crate) fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; labels are not available",
    )
}

pub(crate) fn write_error(e: &LabelWriteError) -> Response {
    match e {
        LabelWriteError::NotFound(m) => error(StatusCode::NOT_FOUND, m.clone()),
        LabelWriteError::Open(m) => error(StatusCode::CONFLICT, m.clone()),
        LabelWriteError::Conflict => error(StatusCode::CONFLICT, "label changed; reload"),
        LabelWriteError::Invalid(m) => error(StatusCode::BAD_REQUEST, m.clone()),
        LabelWriteError::Persist(p) => error(StatusCode::INTERNAL_SERVER_ERROR, p.to_string()),
    }
}

const GOLD_FORBIDDEN: &str = "gold labels are written from the mike UI only; an agent relays the operator's words with relay_label";

fn header_text(headers: &HeaderMap, name: impl header::AsHeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

pub(crate) fn spa_origin(headers: &HeaderMap, state: &AppState) -> Option<SpaOrigin> {
    SpaOrigin::checked(
        header_text(headers, header::ORIGIN).as_deref(),
        header_text(headers, header::HOST).as_deref(),
        header_text(headers, "x-request-id"),
        &state.config.spa_origins,
    )
}

pub(crate) async fn get_vocabulary(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.eval().eval_vocabulary().await {
        Ok(rows) => Json(json!({ "axes": group_vocabulary(rows) })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

fn group_vocabulary(rows: Vec<VocabularyRow>) -> Vec<serde_json::Value> {
    let mut axes: BTreeMap<String, (Vec<String>, Vec<serde_json::Value>)> = BTreeMap::new();
    for row in rows {
        let entry = axes
            .entry(row.axis.clone())
            .or_insert_with(|| (row.subject_kinds.clone(), Vec::new()));
        entry.1.push(json!({"label": row.label, "sort_order": row.sort_order, "description": row.description}));
    }
    axes.into_iter()
        .map(|(axis, (kinds, labels))| json!({"axis": axis, "subject_kinds": kinds, "labels": labels}))
        .collect()
}

#[derive(Debug, Deserialize)]
pub(crate) struct LabelListParams {
    subject_kind: Option<String>,
    turn_id: Option<i64>,
    session: Option<uuid::Uuid>,
    axis: Option<String>,
    tier: Option<String>,
    method: Option<String>,
    split: Option<String>,
    source: Option<String>,
    #[serde(default)]
    current_only: bool,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    before_id: Option<i64>,
    limit: Option<i64>,
}

async fn verdict_for(
    persist: &PersistHandle,
    params: &LabelListParams,
) -> Option<serde_json::Value> {
    if let Some(turn_id) = params.turn_id {
        let kind = params.subject_kind.as_deref().unwrap_or("turn");
        return persist
            .eval()
            .verdict_now(kind, turn_id)
            .await
            .ok()
            .flatten();
    }
    let session = params.session?;
    if params.subject_kind.as_deref() != Some("session_end") {
        return None;
    }
    let ending = persist
        .eval()
        .session_ending_turn(session)
        .await
        .ok()
        .flatten()?;
    persist
        .eval()
        .verdict_now("session_end", ending)
        .await
        .ok()
        .flatten()
}

pub(crate) async fn list_eval_labels(
    State(state): State<AppState>,
    Query(params): Query<LabelListParams>,
) -> Response {
    let limit = params.limit.unwrap_or(100);
    if !(1..=500).contains(&limit) {
        return error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and 500, got {limit}"),
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let query = LabelQuery {
        subject_kind: params.subject_kind.clone(),
        turn_id: params.turn_id,
        session_uuid: params.session,
        axis: params.axis.clone(),
        tiers: params
            .tier
            .as_deref()
            .map(|t| t.split(',').map(|s| s.trim().to_owned()).collect()),
        method: params.method.clone(),
        split: params.split.clone(),
        source: params.source.clone(),
        current_only: params.current_only,
        since: params.since,
        until: params.until,
        before_id: params.before_id,
        limit: limit + 1,
    };
    let mut rows = match persist.eval().list_labels(&query).await {
        Ok(rows) => rows,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let has_more = rows.len() > usize::try_from(limit).unwrap_or(0);
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_before_id = has_more.then(|| rows.last().map(|r| r.label.id)).flatten();
    let verdict_now = verdict_for(persist, &params).await;
    Json(json!({"labels": rows, "next_before_id": next_before_id, "verdict_now": verdict_now}))
        .into_response()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SubjectBody {
    Turn { turn_id: i64 },
    SessionEnd { session_id: uuid::Uuid },
}

impl SubjectBody {
    pub(crate) fn subject(&self) -> Subject {
        match self {
            SubjectBody::Turn { turn_id } => Subject::Turn { turn_id: *turn_id },
            SubjectBody::SessionEnd { session_id } => Subject::SessionEnd {
                session_uuid: *session_id,
            },
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            SubjectBody::Turn { .. } => "turn",
            SubjectBody::SessionEnd { .. } => "session_end",
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GoldBody {
    subject: SubjectBody,
    labels: Vec<GoldEntryBody>,
    note: Option<String>,
    context: Option<String>,
    eval_run_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GoldEntryBody {
    axis: String,
    label: String,
}

pub(crate) fn validate_entry(
    vocab: &[VocabularyRow],
    kind: &str,
    axis: &str,
    label: &str,
) -> Result<(), String> {
    if !matches!(axis, "reaction" | "delivery" | "session_end") {
        return Err(format!("axis {axis:?} cannot be written here"));
    }
    let row = vocab
        .iter()
        .find(|v| v.axis == axis && v.label == label)
        .ok_or_else(|| format!("{axis}={label} is not in the label vocabulary"))?;
    if !row.subject_kinds.iter().any(|k| k == kind) {
        return Err(format!("axis {axis} does not apply to a {kind}"));
    }
    Ok(())
}

fn validate_gold(vocab: &[VocabularyRow], body: &GoldBody) -> Result<Vec<GoldEntry>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in &body.labels {
        validate_entry(vocab, body.subject.kind(), &entry.axis, &entry.label)?;
        if !seen.insert(entry.axis.clone()) {
            return Err(format!("axis {} appears twice", entry.axis));
        }
        out.push(GoldEntry {
            axis: entry.axis.clone(),
            label: entry.label.clone(),
        });
    }
    if out.is_empty() {
        return Err("no labels given".to_owned());
    }
    Ok(out)
}

async fn check_review_run(persist: &PersistHandle, body: &GoldBody) -> Result<(), Box<Response>> {
    let Some(run_id) = body.eval_run_id else {
        return Ok(());
    };
    let SubjectBody::Turn { turn_id } = body.subject else {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            "eval_run_id applies to a turn label only",
        )));
    };
    match persist.eval().eval_run_has_item(run_id, turn_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("turn {turn_id} is not an item of eval run {run_id}"),
        ))),
        Err(e) => Err(Box::new(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            e.to_string(),
        ))),
    }
}

pub(crate) async fn post_gold_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<GoldBody>,
) -> Response {
    let Some(origin) = spa_origin(&headers, &state) else {
        return error(StatusCode::FORBIDDEN, GOLD_FORBIDDEN);
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let vocab = match persist.eval().eval_vocabulary().await {
        Ok(v) => v,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let entries = match validate_gold(&vocab, &body) {
        Ok(entries) => entries,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Err(response) = check_review_run(persist, &body).await {
        return *response;
    }
    let holdout = crate::eval::labels::holdout_percent(persist)
        .await
        .unwrap_or(30);
    let write = GoldWrite {
        subject: body.subject.subject(),
        entries: &entries,
        note: body.note.as_deref(),
        origin: &origin,
        holdout_percent: holdout,
        context: body.context.as_deref().unwrap_or("spa"),
        eval_run_id: body.eval_run_id,
    };
    match persist.eval().write_gold_labels(&write).await {
        Ok(labels) => {
            super::projection::project_after_gold(&state, persist, &labels).await;
            (StatusCode::CREATED, Json(json!({ "labels": labels }))).into_response()
        }
        Err(e) => write_error(&e),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct NoteBody {
    note: Option<String>,
}

pub(crate) async fn withdraw_label(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<NoteBody>>,
) -> Response {
    let Some(origin) = spa_origin(&headers, &state) else {
        return error(StatusCode::FORBIDDEN, GOLD_FORBIDDEN);
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let note = body.and_then(|Json(b)| b.note);
    match persist
        .eval()
        .withdraw_label(id, &origin, note.as_deref())
        .await
    {
        Ok(label) => {
            super::projection::project_after_gold(&state, persist, std::slice::from_ref(&label))
                .await;
            (StatusCode::CREATED, Json(json!({ "label": label }))).into_response()
        }
        Err(e) => write_error(&e),
    }
}

pub(crate) async fn confirm_label(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<NoteBody>>,
) -> Response {
    let Some(origin) = spa_origin(&headers, &state) else {
        return error(StatusCode::FORBIDDEN, GOLD_FORBIDDEN);
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let note = body.and_then(|Json(b)| b.note);
    match persist
        .eval()
        .confirm_relayed_label(id, &origin, note.as_deref())
        .await
    {
        Ok(label) => {
            super::projection::project_after_gold(&state, persist, std::slice::from_ref(&label))
                .await;
            (StatusCode::CREATED, Json(json!({ "label": label }))).into_response()
        }
        Err(e) => write_error(&e),
    }
}

pub(crate) async fn list_import_batches(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.eval().import_batches().await {
        Ok(batches) => Json(json!({ "batches": batches })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
