use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::semantic::KnowledgeState;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{
    KindDefinition, KindError, KindUpdate, KnowledgeKindRow,
};
use crate::web_base::{knowledge_error, persist_unavailable_response};

#[derive(Debug, Serialize)]
pub struct DefinitionView {
    pub source_table: String,
    pub key_column: String,
    pub title_column: String,
    pub body_columns: Vec<String>,
    pub project_column: String,
}

#[derive(Debug, Serialize)]
pub struct WatchView {
    pub table: String,
    pub key_column: String,
    pub watch_columns: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct StatsView {
    pub items: i64,
    pub embedded: i64,
    pub waiting: i64,
    pub failed: i64,
    pub newest_error: Option<String>,
    pub newest_error_at: Option<DateTime<Utc>>,
    pub oldest_waiting_ready_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct KindView {
    pub kind: String,
    pub label: String,
    pub description: Option<String>,
    pub built_in: bool,
    pub enabled: bool,
    pub settle_secs: i32,
    pub body_view: String,
    pub generated_view: bool,
    pub definition: Option<DefinitionView>,
    pub watches: Vec<WatchView>,
    pub stats: StatsView,
    pub behind: bool,
}

#[derive(Deserialize)]
struct StoredWatch {
    table: String,
    key_column: String,
    update_columns: Option<Vec<String>>,
}

fn definition_of(row: &KnowledgeKindRow) -> Option<DefinitionView> {
    Some(DefinitionView {
        source_table: row.source_table.clone()?,
        key_column: row.key_column.clone()?,
        title_column: row.title_column.clone()?,
        body_columns: row.body_columns.clone()?,
        project_column: row.project_column.clone()?,
    })
}

fn watches_of(stored: &Value) -> Vec<WatchView> {
    serde_json::from_value::<Vec<StoredWatch>>(stored.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|watch| WatchView {
            table: watch.table,
            key_column: watch.key_column,
            watch_columns: watch.update_columns.unwrap_or_default(),
        })
        .collect()
}

impl From<KnowledgeKindRow> for KindView {
    fn from(row: KnowledgeKindRow) -> Self {
        Self {
            definition: definition_of(&row),
            watches: watches_of(&row.watches),
            stats: StatsView {
                items: row.items,
                embedded: row.embedded,
                waiting: row.waiting,
                failed: row.failed,
                newest_error: row.newest_error,
                newest_error_at: row.newest_error_at,
                oldest_waiting_ready_at: row.oldest_waiting_ready_at,
            },
            kind: row.kind,
            label: row.label,
            description: row.description,
            built_in: row.built_in,
            enabled: row.enabled,
            settle_secs: row.settle_secs,
            body_view: row.body_view,
            generated_view: row.generated_view,
            behind: row.behind,
        }
    }
}

fn kind_error(error: KindError) -> Response {
    match error {
        KindError::Refused(refusal) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": refusal.message,
                "field": refusal.field,
                "watch_index": Value::Null,
            })),
        )
            .into_response(),
        KindError::Persist(error) => knowledge_error(error),
    }
}

async fn kind_views(state: &KnowledgeState) -> Result<Vec<KindView>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .list_knowledge_kinds()
        .await
        .map(|rows| rows.into_iter().map(KindView::from).collect())
        .map_err(knowledge_error)
}

pub async fn list_kinds(State(state): State<KnowledgeState>) -> Result<Json<Value>, Response> {
    Ok(Json(json!({"kinds": kind_views(&state).await?})))
}

pub async fn list_kind_tables(
    State(state): State<KnowledgeState>,
) -> Result<Json<Value>, Response> {
    let tables = state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .list_knowledge_tables()
        .await
        .map_err(knowledge_error)?;
    Ok(Json(json!({"tables": tables})))
}

pub async fn preview_kind(
    State(state): State<KnowledgeState>,
    Json(definition): Json<KindDefinition>,
) -> Result<Json<Value>, Response> {
    let preview = state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .preview_knowledge_kind(&definition)
        .await
        .map_err(kind_error)?;
    Ok(Json(json!({"preview": preview})))
}

pub async fn define_kind(
    State(state): State<KnowledgeState>,
    Json(definition): Json<KindDefinition>,
) -> Result<(StatusCode, Json<Value>), Response> {
    let kind = state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .define_knowledge_kind(&definition)
        .await
        .map_err(kind_error)?;
    Ok((StatusCode::CREATED, Json(json!({"kind": kind}))))
}

pub async fn patch_kind(
    State(state): State<KnowledgeState>,
    Path(kind): Path<String>,
    Json(update): Json<KindUpdate>,
) -> Result<Json<KindView>, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .update_knowledge_kind(&kind, &update)
        .await
        .map_err(kind_error)?;
    kind_views(&state)
        .await?
        .into_iter()
        .find(|view| view.kind == kind)
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("kind {kind} does not exist"), "field": "kind"})),
            )
                .into_response()
        })
}

pub async fn delete_kind(
    State(state): State<KnowledgeState>,
    Path(kind): Path<String>,
) -> Result<StatusCode, Response> {
    state
        .persist()
        .ok_or_else(persist_unavailable_response)?
        .knowledge()
        .delete_knowledge_kind(&kind)
        .await
        .map_err(kind_error)?;
    Ok(StatusCode::NO_CONTENT)
}
