use super::*;
use crate::store::persistence::project_knowledge::ProjectIdentity;

#[derive(Debug, Deserialize)]
pub(crate) struct RecallSimilarTurnsBody {
    pub(crate) project: String,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
}

pub(crate) const RECALL_SIMILAR_TURNS_DEFAULT_LIMIT: i64 = 5;
pub(crate) const RECALL_SIMILAR_TURNS_MAX_LIMIT: i64 = 50;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SimilarTurnDto {
    #[serde(flatten)]
    pub(crate) turn: OperatorTurnDto,
    pub(crate) rank: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rerank_score: Option<f64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RecallSimilarTurnsResponse {
    pub(crate) turns: Vec<SimilarTurnDto>,
    pub(crate) retrieval_path: &'static str,
    pub(crate) degraded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pending_turns: Option<i64>,
    pub(crate) rerank: crate::api::rerank_search::RerankReport,
}

pub(crate) async fn recall_similar_turns(
    State(state): State<AppState>,
    Json(body): Json<RecallSimilarTurnsBody>,
) -> Result<Json<RecallSimilarTurnsResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    validate_recall_query(&body).map_err(|e| *e)?;
    let limit = body
        .limit
        .map(|l| l as i64)
        .unwrap_or(RECALL_SIMILAR_TURNS_DEFAULT_LIMIT)
        .clamp(1, RECALL_SIMILAR_TURNS_MAX_LIMIT);

    let project = handle
        .resolve_knowledge_project(&body.project)
        .await
        .map_err(crate::api::project_knowledge::knowledge_error)?;
    let (fetch, candidates) = recall_turns_rerank::rows_to_fetch(&state, limit, &body.query);
    let (rows, used_embedding) =
        recall_rows_hybrid(handle, &state, (&project, &body.query), fetch).await?;
    let (ranked, rerank) =
        recall_turns_rerank::rerank_turns(&state, &body.query, rows, candidates).await;

    let degraded = !used_embedding;
    let retrieval_path = if used_embedding { "embedding" } else { "fts" };
    let pending_turns: Option<i64> = if degraded {
        match handle.count_pending_embedding_turns().await {
            Ok(n) => Some(n),
            Err(e) => {
                tracing::warn!(error = %e, "recall_similar_turns: pending count failed");
                None
            }
        }
    } else {
        None
    };

    let turns: Vec<SimilarTurnDto> = ranked
        .into_iter()
        .take(limit as usize)
        .map(|(row, score)| {
            let mut turn = ranked_row_to_similar_turn(row);
            turn.rerank_score = score.map(|s| (f64::from(s) * 10_000.0).round() / 10_000.0);
            turn
        })
        .collect();
    crate::search_events::record_turn_recall(
        &state,
        body.session_id.as_deref(),
        &body.query,
        &turns,
        &rerank,
    );
    Ok(Json(RecallSimilarTurnsResponse {
        turns,
        retrieval_path,
        degraded,
        pending_turns,
        rerank,
    }))
}

const RECALL_QUERY_MAX_CHARS: usize = 4096;

fn validate_recall_query(body: &RecallSimilarTurnsBody) -> Result<(), Box<Response>> {
    if body.project.is_empty() {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "`project` must be non-empty",
                })),
            )
                .into_response(),
        ));
    }
    if body.query.is_empty() {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "`query` must be non-empty",
                })),
            )
                .into_response(),
        ));
    }
    if body.query.chars().count() > RECALL_QUERY_MAX_CHARS {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "query too long",
                    "max_chars": RECALL_QUERY_MAX_CHARS,
                })),
            )
                .into_response(),
        ));
    }
    Ok(())
}

async fn recall_rows_hybrid(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    (project, query): (&ProjectIdentity, &str),
    limit: i64,
) -> Result<(Vec<crate::store::persistence::RankedTurnRow>, bool), Response> {
    let embedding_service = state.embedding_service();
    let rows = if embedding_service.is_available() {
        try_embedding_recall(handle, &embedding_service, project, query, limit).await
    } else {
        None
    };
    let used_embedding = rows.is_some();
    let rows = match rows {
        Some(r) => r,
        None => handle
            .recall_turns_by_fts(project, query, limit)
            .await
            .map_err(|e| {
                tracing::warn!(
                    project = %project.project_path,
                    query = %query,
                    error = %e,
                    "recall_similar_turns: FTS query failed"
                );
                persist_error_response(e)
            })?,
    };
    Ok((rows, used_embedding))
}

async fn try_embedding_recall(
    handle: &crate::store::persistence::PersistHandle,
    embedding_service: &EmbeddingService,
    project: &ProjectIdentity,
    query: &str,
    limit: i64,
) -> Option<Vec<crate::store::persistence::RankedTurnRow>> {
    let query_vec = embed_recall_query(embedding_service, &project.project_path, query).await?;
    let literal = crate::store::operator_turns::vector_to_text_literal(&query_vec);
    let model = embedding_service.model_name();
    recall_by_embedding(handle, project, query, (&model, &literal), limit).await
}

async fn embed_recall_query(
    embedding_service: &EmbeddingService,
    project: &str,
    query: &str,
) -> Option<Vec<f32>> {
    use crate::api::project_knowledge::query_vector_cache::QUERY_VECTORS;
    let key = crate::embed_worker::knowledge::model_key(embedding_service);
    if let Some(cached) = QUERY_VECTORS.get(&key, query) {
        return Some(cached);
    }
    match embedding_service.embed_query(query).await {
        Ok(v) => {
            QUERY_VECTORS.put(&key, query, v.clone());
            Some(v)
        }
        Err(e) => {
            tracing::warn!(
                project,
                error = %e,
                "recall_similar_turns: embedding the query failed; falling back to FTS"
            );
            None
        }
    }
}

async fn recall_by_embedding(
    handle: &crate::store::persistence::PersistHandle,
    project: &ProjectIdentity,
    query: &str,
    (model, literal): (&str, &str),
    limit: i64,
) -> Option<Vec<crate::store::persistence::RankedTurnRow>> {
    let path = project.project_path.as_str();
    match handle
        .recall_turns_by_embedding(project, model, literal, limit)
        .await
    {
        Ok(Some(found)) => {
            log_embedding_recall_hit(path, found.len());
            Some(found)
        }
        Ok(None) => {
            log_embedding_recall_empty(path);
            None
        }
        Err(e) => {
            log_embedding_recall_query_failed(path, query, &e);
            None
        }
    }
}

fn log_embedding_recall_hit(project: &str, rows: usize) {
    tracing::debug!(project, rows, "recall_similar_turns: embedding path");
}

fn log_embedding_recall_empty(project: &str) {
    tracing::debug!(
        project,
        "recall_similar_turns: no embedded rows in project; falling back to FTS"
    );
}

fn log_embedding_recall_query_failed(
    project: &str,
    query: &str,
    e: &crate::store::persistence::PersistError,
) {
    tracing::warn!(
        project,
        query,
        error = %e,
        "recall_similar_turns: embedding query failed; falling back to FTS"
    );
}

fn ranked_row_to_similar_turn(r: crate::store::persistence::RankedTurnRow) -> SimilarTurnDto {
    SimilarTurnDto {
        rank: r.rank,
        rerank_score: None,
        turn: OperatorTurnDto::from(crate::store::operator_turns::OperatorTurnRow {
            turn_id: r.turn_id,
            session_id: r.session_id,
            project_id: r.project_id,
            start_request_id: r.start_request_id,
            end_request_id: r.end_request_id,
            start_request_number: 0,
            end_request_number: 0,
            user_prompt: r.user_prompt,
            final_assistant_text: r.final_assistant_text,
            tool_call_count: r.tool_call_count,
            tool_error_count: r.tool_error_count,
            files_touched: r.files_touched,
            tools_used: r.tools_used,
            agents_dispatched: r.agents_dispatched,
            skills_used: Vec::new(),
            weasel_findings_count: r.weasel_findings_count,
            total_input_tokens: r.total_input_tokens,
            total_output_tokens: r.total_output_tokens,
            duration_ms: r.duration_ms,
            stop_response: None,
            stop_reason: None,
            stop_request_id: None,
            interrupted: r.interrupted,
            counts_as_correction: r.outcome == "corrected",
            outcome: r.outcome,
            outcome_evidence: r.outcome_evidence,
            classified_at: r.classified_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            prompt_kind: None,
            subject_command: None,
            subject_agent: None,
            model: None,
            outcome_bundle_id: None,
            reaction: None,
            delivery: None,
            outcome_source: None,
            live_verdict_id: None,
            outcome_projection_version: None,
        }),
    }
}
