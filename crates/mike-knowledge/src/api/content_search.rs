use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::semantic::KnowledgeState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::content_search::{
    ContentQuery, ContentQueryError, ContentSearchRow, MAX_QUERY_WORDS, PageRequest, SearchMode,
    SessionOrder,
};

const CONTENT_SEARCH_MAX_QUERY_BYTES: usize = 8192;
const DEFAULT_SESSION_LIMIT: u32 = 20;
const MAX_SESSION_LIMIT: u32 = 100;
const MAX_MATCHES_PER_SESSION: usize = 50;

#[derive(Debug, Deserialize)]
pub struct ContentSearchParams {
    q: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    caller_session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContentSearchMatchDto {
    turn_id: i64,
    request_id: i64,
    role: String,
    chunk_idx: i32,
    byte_offset: u64,
    match_length: u32,
    line: u32,
    column: u32,
    before: String,
    matched: String,
    after: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ContentSearchSessionDto {
    session_id: String,
    project_path: String,
    project_display_name: String,
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    words_matched: Option<u64>,
    match_count: u64,
    prompt_match_count: u64,
    turn_count: u64,
    last_match_at: DateTime<Utc>,
    matches: Vec<ContentSearchMatchDto>,
    matches_truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct ContentSearchResponse {
    query: String,
    mode: &'static str,
    order: &'static str,
    total_matches: u64,
    total_sessions: u64,
    sessions: Vec<ContentSearchSessionDto>,
    has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ContentSearchErrorBody {
    error: String,
}

fn content_search_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ContentSearchErrorBody {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

enum QueryValidationError {
    Empty,
    TooLong(usize),
}

fn query_validation_error_response(error: QueryValidationError) -> Response {
    match error {
        QueryValidationError::Empty => {
            content_search_error(StatusCode::BAD_REQUEST, "q is required")
        }
        QueryValidationError::TooLong(len) => content_search_error(
            StatusCode::BAD_REQUEST,
            &format!("q exceeds {CONTENT_SEARCH_MAX_QUERY_BYTES} bytes (got {len})"),
        ),
    }
}

fn validate_query(q: &str) -> Result<(), QueryValidationError> {
    if q.is_empty() {
        return Err(QueryValidationError::Empty);
    }
    if q.len() > CONTENT_SEARCH_MAX_QUERY_BYTES {
        return Err(QueryValidationError::TooLong(q.len()));
    }
    Ok(())
}

fn parse_order(order: Option<&str>) -> Result<SessionOrder, Box<Response>> {
    let Some(order) = order else {
        return Ok(SessionOrder::Matches);
    };
    SessionOrder::parse(order).ok_or_else(|| {
        Box::new(content_search_error(
            StatusCode::BAD_REQUEST,
            &format!("order must be `matches` or `recent` (got `{order}`)"),
        ))
    })
}

fn parse_query(q: &str, mode: Option<&str>) -> Result<ContentQuery, Box<Response>> {
    let mode = match mode {
        None => SearchMode::Phrase,
        Some(value) => SearchMode::parse(value).ok_or_else(|| {
            Box::new(content_search_error(
                StatusCode::BAD_REQUEST,
                &format!("mode must be `phrase`, `all` or `any` (got `{value}`)"),
            ))
        })?,
    };
    ContentQuery::new(q, mode).map_err(|error| {
        let message = match error {
            ContentQueryError::NoWords => "q has no words".to_owned(),
            ContentQueryError::TooManyWords { count, .. } => format!(
                "mode `{}` accepts at most {MAX_QUERY_WORDS} words in q (got {count})",
                mode.as_str()
            ),
        };
        Box::new(content_search_error(StatusCode::BAD_REQUEST, &message))
    })
}

async fn resolve_project(
    handle: &PersistHandle,
    selector: Option<&str>,
) -> Result<Option<i64>, Response> {
    let Some(selector) = selector.filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    handle
        .knowledge()
        .resolve_knowledge_project(selector)
        .await
        .map(|project| Some(project.id))
        .map_err(crate::api::project_knowledge::knowledge_error)
}

async fn fetch_search_totals(
    handle: &PersistHandle,
    query: &ContentQuery,
    project_id: Option<i64>,
) -> Result<crate::store::persistence::content_search::ContentSearchTotals, Response> {
    handle
        .search_content_totals(query, project_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "content_search: totals query failed");
            content_search_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })
}

async fn fetch_search_page(
    handle: &PersistHandle,
    query: &ContentQuery,
    project_id: Option<i64>,
    page: &PageRequest,
) -> Result<Vec<ContentSearchRow>, Response> {
    handle
        .search_content_page(query, project_id, page)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "content_search: page query failed");
            content_search_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })
}

fn push_new_session_if_needed(
    sessions: &mut Vec<ContentSearchSessionDto>,
    row: &ContentSearchRow,
    query: &ContentQuery,
) {
    let is_new_session = sessions
        .last()
        .is_none_or(|s| s.session_id != row.session_id);
    if !is_new_session {
        return;
    }
    sessions.push(ContentSearchSessionDto {
        session_id: row.session_id.clone(),
        project_path: row.project_path.clone(),
        project_display_name: row.project_display_name.clone(),
        title: row.title.clone(),
        words_matched: (query.mode() != SearchMode::Phrase)
            .then_some(row.words_matched.max(0) as u64),
        match_count: row.match_count.max(0) as u64,
        prompt_match_count: row.prompt_match_count.max(0) as u64,
        turn_count: row.turn_count.max(0) as u64,
        last_match_at: row.last_match_at,
        matches: Vec::new(),
        matches_truncated: row.match_count.max(0) as usize > MAX_MATCHES_PER_SESSION,
    });
}

fn first_match_in_chunk(
    chunk_text: &str,
    query: &ContentQuery,
) -> Option<crate::web_base::body_search::TextMatch> {
    query
        .words()
        .iter()
        .filter_map(|word| {
            crate::web_base::body_search::find_in_str(chunk_text, word, false, 120, 1)
                .1
                .into_iter()
                .next()
        })
        .min_by_key(|m| m.byte_offset)
}

fn append_match_if_room(
    sessions: &mut [ContentSearchSessionDto],
    row: ContentSearchRow,
    query: &ContentQuery,
) {
    let Some(current) = sessions.last_mut() else {
        return;
    };
    if current.matches.len() >= MAX_MATCHES_PER_SESSION {
        return;
    }
    let Some(m) = first_match_in_chunk(&row.chunk_text, query) else {
        return;
    };
    current.matches.push(ContentSearchMatchDto {
        turn_id: row.turn_id,
        request_id: row.request_id,
        role: row.role,
        chunk_idx: row.chunk_idx,
        byte_offset: m.byte_offset,
        match_length: m.match_length,
        line: m.line,
        column: m.column,
        before: m.before,
        matched: m.matched,
        after: m.after,
        created_at: row.created_at,
    });
}

fn group_rows_by_session(
    rows: Vec<ContentSearchRow>,
    query: &ContentQuery,
) -> Vec<ContentSearchSessionDto> {
    let mut sessions: Vec<ContentSearchSessionDto> = Vec::new();
    for row in rows {
        push_new_session_if_needed(&mut sessions, &row, query);
        append_match_if_room(&mut sessions, row, query);
    }
    sessions
}

pub async fn search_content(
    State(state): State<KnowledgeState>,
    Query(params): Query<ContentSearchParams>,
) -> Response {
    let caller = params.caller_session_id.clone();
    match run_search(&state, params).await {
        Ok(answer) => {
            record_answer(&state, caller.as_deref(), &answer);
            Json(answer).into_response()
        }
        Err(response) => response,
    }
}

fn record_answer(state: &KnowledgeState, caller: Option<&str>, answer: &ContentSearchResponse) {
    let sessions: Vec<(String, u64)> = answer
        .sessions
        .iter()
        .map(|session| (session.session_id.clone(), session.match_count))
        .collect();
    crate::search_events::record_conversation_search(state, caller, &answer.query, &sessions);
}

async fn run_search(
    state: &KnowledgeState,
    params: ContentSearchParams,
) -> Result<ContentSearchResponse, Response> {
    validate_query(&params.q).map_err(query_validation_error_response)?;
    let order = parse_order(params.order.as_deref()).map_err(|response| *response)?;
    let query = parse_query(&params.q, params.mode.as_deref()).map_err(|response| *response)?;
    let handle = state.persist().ok_or_else(|| {
        content_search_error(StatusCode::SERVICE_UNAVAILABLE, "persistence unavailable")
    })?;
    let limit = params
        .limit
        .unwrap_or(DEFAULT_SESSION_LIMIT)
        .clamp(1, MAX_SESSION_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let project_id = resolve_project(handle, params.project.as_deref()).await?;
    let totals = fetch_search_totals(handle, &query, project_id).await?;
    let page = PageRequest {
        order,
        limit: i64::from(limit),
        offset: i64::from(offset),
    };
    let rows = fetch_search_page(handle, &query, project_id, &page).await?;
    let sessions = group_rows_by_session(rows, &query);

    let returned_sessions = sessions.len() as u32;
    let has_more =
        u64::from(offset) + u64::from(returned_sessions) < totals.total_sessions.max(0) as u64;
    let next_offset = has_more.then_some(offset + returned_sessions);

    Ok(ContentSearchResponse {
        query: params.q,
        mode: query.mode().as_str(),
        order: order.as_str(),
        total_matches: totals.total_matches.max(0) as u64,
        total_sessions: totals.total_sessions.max(0) as u64,
        sessions,
        has_more,
        next_offset,
    })
}
