use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::project_knowledge::{
    KnowledgeQuery, LocationInput, ScanInput, SourceInput,
};
use crate::web_base::detail::Detail;
use crate::web_base::persist_unavailable_response;
use crate::web_base::result_order::ResultOrder;

pub use crate::web_base::knowledge_error;

#[derive(Deserialize, Default)]
pub struct LocationsQuery {
    #[serde(default)]
    include_all: bool,
}

pub async fn locations(
    State(state): State<AppState>,
    Query(query): Query<LocationsQuery>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .list_project_locations(query.include_all)
        .await
        .map(Json)
        .map_err(knowledge_error)
}

pub async fn register_location(
    State(state): State<AppState>,
    Json(input): Json<LocationInput>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .register_project_location(&input)
        .await
        .map(Json)
        .map_err(knowledge_error)
}

pub async fn register_source(
    State(state): State<AppState>,
    Json(input): Json<SourceInput>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .register_knowledge_source(&input)
        .await
        .map(Json)
        .map_err(knowledge_error)
}

pub async fn scan(
    State(state): State<AppState>,
    Path(source): Path<Uuid>,
    Json(input): Json<ScanInput>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .ingest_knowledge_scan(source, &input)
        .await
        .map(Json)
        .map_err(knowledge_error)
}

#[derive(Deserialize, Default)]
pub struct CallerSession {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

pub async fn search(
    State(state): State<AppState>,
    Query(input): Query<KnowledgeQuery>,
    Query(caller): Query<CallerSession>,
) -> Result<Json<Value>, Response> {
    crate::metrics::time_phase(
        "knowledge_search_handler",
        answer_search(state, input, caller),
    )
    .await
}

async fn answer_search(
    state: AppState,
    input: KnowledgeQuery,
    caller: CallerSession,
) -> Result<Json<Value>, Response> {
    let order = ResultOrder::parse(caller.order.as_deref()).map_err(|refusal| *refusal)?;
    let detail = Detail::parse(caller.detail.as_deref()).map_err(|refusal| *refusal)?;
    if let Err(refusal) = input.window() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": refusal}))).into_response());
    }
    let input = KnowledgeQuery {
        caller_session: caller.session_id.clone(),
        recent: order.is_recent(),
        ..input
    };
    let found = retrieve_page(&state, &input).await?;
    let mut log = crate::search_events::SearchLog::knowledge(&input, &found.answer);
    let mut answer = found.answer;
    if detail.is_best() {
        best_shape::best_only(&mut answer, &found.passages);
        log = log.delivered_as(&answer);
    }
    log.record(&state, caller.session_id.as_deref());
    Ok(Json(answer))
}

#[derive(Deserialize)]
pub struct ReadQuery {
    #[serde(default)]
    project: Option<String>,
    reference: String,
}

pub async fn read(
    State(state): State<AppState>,
    Query(input): Query<ReadQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    let named = input.project.as_deref().filter(|p| !p.trim().is_empty());
    let mut read = read_outside::read(&state, handle, named, &input.reference).await?;
    disk_check::annotate(&mut read).await;
    Ok(Json(read))
}

#[derive(Deserialize)]
pub struct LessonFilesQuery {
    location_uuid: Uuid,
}

pub async fn lesson_files(
    State(state): State<AppState>,
    Query(input): Query<LessonFilesQuery>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .lesson_files(input.location_uuid)
        .await
        .map(Json)
        .map_err(knowledge_error)
}

pub async fn import_lesson(
    State(state): State<AppState>,
    Json(input): Json<crate::store::persistence::project_knowledge::LessonImport>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .import_lesson_file(&input)
        .await
        .map(Json)
        .map_err(knowledge_error)
}

#[derive(Deserialize)]
pub struct LessonReceipt {
    location_uuid: Uuid,
    receipts: Value,
}

pub async fn acknowledge_lessons(
    State(state): State<AppState>,
    Json(input): Json<LessonReceipt>,
) -> Result<Json<Value>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .acknowledge_lesson_files(input.location_uuid, input.receipts)
        .await
        .map_err(knowledge_error)?;
    Ok(Json(json!({"acknowledged": true})))
}

#[derive(Deserialize)]
pub struct DefineProject {
    name: String,
    project_uuid: Option<Uuid>,
}

pub async fn define_project(
    State(state): State<AppState>,
    Json(input): Json<DefineProject>,
) -> Result<Json<Value>, Response> {
    let project = state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .define_knowledge_project(&input.name, input.project_uuid.unwrap_or_else(Uuid::new_v4))
        .await
        .map_err(knowledge_error)?;
    Ok(Json(json!(project)))
}

pub mod best_lean;
pub mod best_shape;
pub mod disk_check;
pub mod health;
pub mod item_header;
pub mod passage_excerpt;
pub mod query_vector_cache;
pub mod read_outside;
pub mod rerank_page;
pub mod rerank_pass;
pub use rerank_pass::rerank_pass;
pub mod recent;
pub mod rescue;
pub mod retrieval;
pub mod wide;
#[cfg(test)]
pub mod wide_tests;
pub use health::embedding_capability;
pub use retrieval::{retrieve_page, retrieve_without_rerank};

#[cfg(test)]
pub mod disk_check_tests;
#[cfg(test)]
pub mod query_vector_cache_tests;
#[cfg(test)]
pub mod rescue_tests;
