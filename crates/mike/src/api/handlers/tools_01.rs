use super::*;
use crate::store::persistence::steerer_repo::SteererPersist as _;

pub(crate) const TOOL_USES_DEFAULT_LIMIT: usize = 200;

pub(crate) const TOOL_USES_MAX_LIMIT: usize = 1000;

#[derive(Debug, Deserialize)]
pub(crate) struct ToolUsesParams {
    #[serde(default)]
    pub(crate) offset: Option<usize>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) order: Option<SortOrder>,
    #[serde(default = "default_include_results")]
    pub(crate) include_results: bool,
    #[serde(default)]
    pub(crate) agent: Option<String>,
}

pub(crate) fn default_include_results() -> bool {
    true
}

impl Default for ToolUsesParams {
    fn default() -> Self {
        Self {
            offset: None,
            limit: None,
            order: None,
            include_results: true,
            agent: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolUseDto {
    pub(crate) use_id: i64,
    pub(crate) request_id: i64,
    pub(crate) session_id: String,
    pub(crate) created_at: String,
    pub(crate) tool_id: String,
    pub(crate) tool_name: String,
    pub(crate) input: serde_json::Value,
    pub(crate) file_path: Option<String>,
    pub(crate) ord: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_instance_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subagent_persona: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<ToolResultDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolUsesResponse {
    pub(crate) session_id: String,
    pub(crate) total_count: i64,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) rows: Vec<ToolUseDto>,
}

fn tool_uses_query_error(
    session_id: &str,
    operation: &'static str,
    error: crate::store::persistence::PersistError,
) -> Response {
    tracing::warn!(
        error = %error,
        session_id,
        operation,
        "list_session_tool_uses query failed",
    );
    (StatusCode::INTERNAL_SERVER_ERROR, "tool_uses query failed").into_response()
}

async fn resolve_tool_use_session(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
) -> Result<Option<i64>, Response> {
    handle
        .resolve_session_db_id(session_id)
        .await
        .map_err(|error| tool_uses_query_error(session_id, "resolve", error))
}

fn tool_use_dto(
    row: crate::store::persistence::ToolUseRow,
    result: Option<crate::store::persistence::BatchToolResultRow>,
    include_results: bool,
) -> ToolUseDto {
    let result = result.map(|result| ToolResultDto {
        tool_use_id: result.wire_tool_use_id,
        is_error: result.is_error,
        content_sha256: result.content_sha256,
        content_bytes: result.content_bytes.max(0) as u64,
        content: result.content,
        request_id: Some(result.request_id),
    });
    let result_bytes = result.as_ref().map(|result| result.content_bytes);
    let is_error = result.as_ref().map(|result| result.is_error);
    ToolUseDto {
        use_id: row.tu_id,
        request_id: row.request_id,
        session_id: row.session_id,
        created_at: row.created_at.to_rfc3339(),
        tool_id: row.tool_id,
        tool_name: row.tool_name,
        input: row.input.unwrap_or(serde_json::Value::Null),
        file_path: row.file_path,
        ord: row.ord,
        agent_instance_id: row.agent_instance_id,
        agent_name: row.agent_name,
        subagent_persona: row.subagent_persona,
        result_bytes,
        is_error,
        result: include_results.then_some(result).flatten(),
    }
}

pub(crate) async fn list_session_tool_uses(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ToolUsesParams>,
) -> Result<Json<ToolUsesResponse>, Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let offset = params.offset.unwrap_or(0);
    let limit = params
        .limit
        .unwrap_or(TOOL_USES_DEFAULT_LIMIT)
        .clamp(1, TOOL_USES_MAX_LIMIT);

    let session_db_id = match resolve_tool_use_session(handle, &id).await? {
        Some(db_id) => db_id,
        None => {
            return Ok(Json(ToolUsesResponse {
                session_id: id,
                total_count: 0,
                offset,
                limit,
                rows: vec![],
            }));
        }
    };

    let total_count = handle
        .steerer()
        .count_tool_uses_for_session(session_db_id, params.agent.as_deref())
        .await
        .map_err(|error| tool_uses_query_error(&id, "count", error))?;

    let rows = handle
        .steerer()
        .fetch_tool_uses_for_session(
            session_db_id,
            offset as i64,
            limit as i64,
            params.order.unwrap_or_default(),
            params.agent.as_deref(),
        )
        .await
        .map_err(|error| tool_uses_query_error(&id, "page", error))?;

    let tu_ids: Vec<i64> = rows.iter().map(|r| r.tu_id).collect();
    let results_by_tu_id = handle
        .steerer()
        .fetch_results_for_tool_use_ids(&tu_ids, session_db_id)
        .await
        .map_err(|error| tool_uses_query_error(&id, "results", error))?;
    use std::collections::HashMap;
    let mut result_map: HashMap<i64, _> = HashMap::with_capacity(results_by_tu_id.len());
    for r in results_by_tu_id {
        result_map.insert(r.tu_id, r);
    }

    let include_results = params.include_results;
    let dto_rows: Vec<ToolUseDto> = rows
        .into_iter()
        .map(|row| {
            let result = result_map.remove(&row.tu_id);
            tool_use_dto(row, result, include_results)
        })
        .collect();

    Ok(Json(ToolUsesResponse {
        session_id: id,
        total_count,
        offset,
        limit,
        rows: dto_rows,
    }))
}

pub(crate) const TOOL_USE_STATS_DEFAULT_TOP_N: i64 = 20;
pub(crate) const TOOL_USE_STATS_MAX_TOP_N: i64 = 100;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ToolUseStatsParams {
    #[serde(default)]
    pub(crate) top_n: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToolNameStatDto {
    pub(crate) tool_name: String,
    pub(crate) use_count: i64,
    pub(crate) total_result_bytes: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct DuplicateInputGroupDto {
    pub(crate) tool_name: String,
    pub(crate) input_digest: String,
    pub(crate) count: i64,
    pub(crate) wasted_bytes: i64,
    pub(crate) sample_request_ids: Vec<i64>,
    pub(crate) input_excerpt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopResultDto {
    pub(crate) tool_name: String,
    pub(crate) request_id: i64,
    pub(crate) tool_id: String,
    pub(crate) result_bytes: i64,
    pub(crate) input_excerpt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionToolUseStatsResponse {
    pub(crate) session_id: String,
    pub(crate) by_tool: Vec<ToolNameStatDto>,
    pub(crate) duplicate_input_groups: Vec<DuplicateInputGroupDto>,
    pub(crate) top_results: Vec<TopResultDto>,
}

pub(crate) async fn session_tool_use_stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ToolUseStatsParams>,
) -> Result<Json<SessionToolUseStatsResponse>, Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let top_n = params
        .top_n
        .unwrap_or(TOOL_USE_STATS_DEFAULT_TOP_N)
        .clamp(1, TOOL_USE_STATS_MAX_TOP_N);

    let session_db_id = match handle.resolve_session_db_id(&id).await {
        Ok(Some(db_id)) => db_id,
        Ok(None) => {
            return Ok(Json(SessionToolUseStatsResponse {
                session_id: id,
                by_tool: vec![],
                duplicate_input_groups: vec![],
                top_results: vec![],
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, session_id = %id, "session_tool_use_stats: resolve failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "tool_use_stats query failed",
            )
                .into_response());
        }
    };

    let by_tool_rows = handle
        .fetch_session_tool_stats_by_tool(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "session_tool_use_stats: by-tool query failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "tool_use_stats query failed").into_response()
        })?;
    let dup_rows = handle
        .fetch_session_duplicate_input_groups(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "session_tool_use_stats: duplicate-group query failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "tool_use_stats query failed").into_response()
        })?;
    let top_rows = handle
        .fetch_session_top_results(session_db_id, top_n)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "session_tool_use_stats: top-results query failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "tool_use_stats query failed").into_response()
        })?;

    let by_tool = by_tool_rows
        .into_iter()
        .map(|r| ToolNameStatDto {
            tool_name: r.tool_name,
            use_count: r.use_count,
            total_result_bytes: r.total_result_bytes,
        })
        .collect();

    let duplicate_input_groups = dup_rows
        .into_iter()
        .map(|r| {
            let wasted_bytes = (((r.use_count - 1) as f64) * r.avg_result_bytes).round() as i64;
            DuplicateInputGroupDto {
                tool_name: r.tool_name,
                input_digest: r.input_digest,
                count: r.use_count,
                wasted_bytes,
                sample_request_ids: r.sample_request_ids,
                input_excerpt: r.input_excerpt.unwrap_or_default(),
            }
        })
        .collect();

    let top_results = top_rows
        .into_iter()
        .map(|r| TopResultDto {
            tool_name: r.tool_name,
            request_id: r.request_id,
            tool_id: r.tool_id,
            result_bytes: r.content_bytes,
            input_excerpt: r.input_excerpt.unwrap_or_default(),
        })
        .collect();

    Ok(Json(SessionToolUseStatsResponse {
        session_id: id,
        by_tool,
        duplicate_input_groups,
        top_results,
    }))
}

#[derive(Serialize)]
pub(crate) struct SessionDedupPathStatsDto {
    pub(crate) file_path: String,
    pub(crate) hit_count: u64,
    pub(crate) would_save_bytes: u64,
}

#[derive(Serialize)]
pub(crate) struct SessionDedupStatsDto {
    pub(crate) session_id: String,
    pub(crate) hit_count: u64,
    pub(crate) would_save_bytes: u64,
    pub(crate) read_bytes_total: u64,
    pub(crate) per_path: Vec<SessionDedupPathStatsDto>,
}

pub(crate) async fn get_session_dedup_stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionDedupStatsDto>, Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    let stats = state.store.read().await.dedup_stats(&id);
    let mut per_path: Vec<SessionDedupPathStatsDto> = stats
        .per_path
        .into_iter()
        .map(|(file_path, s)| SessionDedupPathStatsDto {
            file_path,
            hit_count: s.hit_count,
            would_save_bytes: s.would_save_bytes,
        })
        .collect();
    per_path.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    Ok(Json(SessionDedupStatsDto {
        session_id: id,
        hit_count: stats.hit_count,
        would_save_bytes: stats.would_save_bytes,
        read_bytes_total: stats.read_bytes_total,
        per_path,
    }))
}
