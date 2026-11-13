use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

#[derive(Debug, Serialize)]
pub struct CompositionUsageSampleDto {
    pub request_id: i64,
    pub session_id: String,
    pub created_at: String,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CompositionUsageStatsDto {
    pub prompt_sha256: String,
    pub count_1h: i64,
    pub count_24h: i64,
    pub count_7d: i64,
    pub count_total: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub recent: Vec<CompositionUsageSampleDto>,
    pub agent: Option<AgentDto>,
}

pub async fn get_composition_stats(
    State(state): State<AppState>,
    Path(sha): Path<String>,
) -> Result<Json<CompositionUsageStatsDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; usage stats are not available",
        ));
    };
    parse_prompt_sha256_path(&sha)?;
    let composition = handle
        .proxy()
        .fetch_composition_by_prompt_sha256(&sha)
        .await
        .map_err(map_composition_persist_error)?;
    let stats = match composition.as_ref() {
        Some(c) => handle
            .composition_usage_stats(c.id, 50)
            .await
            .map_err(map_composition_persist_error)?,
        None => Default::default(),
    };
    let agent_dto = match composition.as_ref().and_then(|c| c.agent_name.as_deref()) {
        Some(name) => handle
            .fetch_agent_by_name(name)
            .await
            .map_err(map_composition_persist_error)?
            .map(|a| agent_to_dto(&a)),
        None => None,
    };
    let recent = stats
        .recent
        .into_iter()
        .map(|s| CompositionUsageSampleDto {
            request_id: s.request_id,
            session_id: s.session_id,
            created_at: s.created_at.to_rfc3339(),
            model: s.model,
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            cache_read_tokens: s.cache_read_tokens,
            cache_write_tokens: s.cache_write_tokens,
        })
        .collect();
    Ok(Json(CompositionUsageStatsDto {
        prompt_sha256: sha,
        count_1h: stats.count_1h,
        count_24h: stats.count_24h,
        count_7d: stats.count_7d,
        count_total: stats.count_total,
        total_input_tokens: stats.total_input_tokens,
        total_output_tokens: stats.total_output_tokens,
        recent,
        agent: agent_dto,
    }))
}
