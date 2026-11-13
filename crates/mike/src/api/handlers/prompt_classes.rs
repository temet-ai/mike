use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

pub(crate) const PROMPT_CLASSES_DEFAULT_LIMIT: i64 = 50;
pub(crate) const PROMPT_CLASSES_MAX_LIMIT: i64 = 500;
pub(crate) const PROMPT_CLASS_OBSERVATIONS_DEFAULT_LIMIT: i64 = 20;
pub(crate) const PROMPT_CLASS_OBSERVATIONS_MAX_LIMIT: i64 = 200;

pub(crate) const SUBAGENT_PERSONA_UNRESOLVED_SEGMENT: &str = "unresolved";

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PromptClassesParams {
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
}

impl PromptClassesParams {
    fn project_path_filter(&self) -> Option<&str> {
        self.project_path.as_deref().filter(|p| !p.is_empty())
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PromptClassDetailParams {
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
}

impl PromptClassDetailParams {
    fn model_filter(&self) -> Option<&str> {
        self.model.as_deref().filter(|m| !m.is_empty())
    }

    fn project_path_filter(&self) -> Option<&str> {
        self.project_path.as_deref().filter(|p| !p.is_empty())
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptClassModelCountDto {
    pub(crate) model: Option<String>,
    pub(crate) count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptClassDto {
    pub(crate) lane: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) observation_count: i64,
    pub(crate) first_seen: Option<String>,
    pub(crate) last_seen: Option<String>,
    pub(crate) block_count: u32,
    pub(crate) model_breakdown: Vec<PromptClassModelCountDto>,
    pub(crate) registration: PromptRegistrationDto,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptRegistrationDto {
    pub(crate) registered: bool,
    pub(crate) composition_id: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) agent_definition_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptClassDetailDto {
    pub(crate) lane: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) observation_count: i64,
    pub(crate) first_seen: Option<String>,
    pub(crate) last_seen: Option<String>,
    pub(crate) sample_blocks: Option<serde_json::Value>,
    pub(crate) model_breakdown: Vec<PromptClassModelCountDto>,
    pub(crate) registration: PromptRegistrationDto,
    pub(crate) recent_observations: Vec<PromptClassObservationDto>,
    pub(crate) observations_total_count: i64,
    pub(crate) observations_offset: i64,
    pub(crate) observations_limit: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptClassObservationDto {
    pub(crate) request_id: String,
    pub(crate) session_id: String,
    pub(crate) created_at: String,
    pub(crate) content_sha256: Option<String>,
    pub(crate) altered: bool,
    pub(crate) model: Option<String>,
}

pub(crate) fn prompt_registration_dto(
    composition_id: Option<i64>,
    label: Option<String>,
    mode: Option<String>,
    agent_definition_path: Option<String>,
) -> PromptRegistrationDto {
    PromptRegistrationDto {
        registered: composition_id.is_some(),
        composition_id: composition_id.map(|id| id.to_string()),
        label,
        mode,
        agent_definition_path,
    }
}

pub(crate) fn prompt_class_model_breakdown_dto(
    breakdown: sqlx::types::Json<
        Vec<crate::store::persistence::compositions::PromptClassModelCount>,
    >,
) -> Vec<PromptClassModelCountDto> {
    breakdown
        .0
        .into_iter()
        .map(|m| PromptClassModelCountDto {
            model: m.model,
            count: m.count,
        })
        .collect()
}

pub(crate) async fn list_prompt_classes(
    State(state): State<AppState>,
    Query(params): Query<PromptClassesParams>,
) -> Result<Json<Vec<PromptClassDto>>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Ok(Json(Vec::new()));
    };
    let window_start = parse_rfc3339_query_date(params.date_from.as_deref(), "date_from");
    let window_end = parse_rfc3339_query_date(params.date_to.as_deref(), "date_to");
    let limit = params
        .limit
        .unwrap_or(PROMPT_CLASSES_DEFAULT_LIMIT)
        .clamp(1, PROMPT_CLASSES_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).max(0);
    let rows = handle
        .proxy()
        .list_prompt_classes(
            window_start,
            window_end,
            limit,
            offset,
            params.project_path_filter(),
        )
        .await
        .map_err(map_composition_persist_error)?;
    let classes: Vec<PromptClassDto> = rows
        .into_iter()
        .map(|row| PromptClassDto {
            lane: row.lane,
            subagent_persona: row.subagent_persona,
            observation_count: row.observation_count,
            first_seen: row.first_seen.map(|dt| dt.to_rfc3339()),
            last_seen: row.last_seen.map(|dt| dt.to_rfc3339()),
            block_count: u32::try_from(row.block_count).unwrap_or(0),
            model_breakdown: prompt_class_model_breakdown_dto(row.model_breakdown),
            registration: prompt_registration_dto(
                row.composition_id,
                row.label,
                row.mode,
                row.agent_definition_path,
            ),
        })
        .collect();
    Ok(Json(classes))
}

pub(crate) async fn prompt_class_detail_response(
    state: &AppState,
    lane: &str,
    persona: Option<&str>,
    params: PromptClassDetailParams,
) -> Result<Json<PromptClassDetailDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; prompt-class detail is not available",
        ));
    };
    let window_start = parse_rfc3339_query_date(params.date_from.as_deref(), "date_from");
    let window_end = parse_rfc3339_query_date(params.date_to.as_deref(), "date_to");
    let scope = crate::store::persistence::compositions::PromptClassScope {
        model: params.model_filter(),
        project_path: params.project_path_filter(),
    };
    let Some(header) = handle
        .proxy()
        .get_prompt_class_detail(lane, persona, window_start, window_end, scope)
        .await
        .map_err(map_composition_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no prompt class observed or registered for that (lane, persona)",
        ));
    };
    let obs_limit = params
        .limit
        .unwrap_or(PROMPT_CLASS_OBSERVATIONS_DEFAULT_LIMIT)
        .clamp(1, PROMPT_CLASS_OBSERVATIONS_MAX_LIMIT);
    let obs_offset = params.offset.unwrap_or(0).max(0);
    let observation_rows = handle
        .proxy()
        .list_prompt_class_observations(lane, persona, obs_limit, obs_offset, scope)
        .await
        .map_err(map_composition_persist_error)?;
    let observations_total_count = observation_rows.first().map_or(0, |r| r.total_count);
    let recent_observations: Vec<PromptClassObservationDto> = observation_rows
        .into_iter()
        .map(|row| PromptClassObservationDto {
            request_id: row.request_id.to_string(),
            session_id: row.session_id.to_string(),
            created_at: row.created_at.to_rfc3339(),
            content_sha256: row.content_sha256,
            altered: row.altered,
            model: row.model,
        })
        .collect();
    Ok(Json(PromptClassDetailDto {
        lane: header.lane,
        subagent_persona: header.subagent_persona,
        observation_count: header.observation_count,
        first_seen: header.first_seen.map(|dt| dt.to_rfc3339()),
        last_seen: header.last_seen.map(|dt| dt.to_rfc3339()),
        sample_blocks: header.sample_blocks,
        model_breakdown: prompt_class_model_breakdown_dto(header.model_breakdown),
        registration: prompt_registration_dto(
            header.composition_id,
            header.label,
            header.mode,
            header.agent_definition_path,
        ),
        recent_observations,
        observations_total_count,
        observations_offset: obs_offset,
        observations_limit: obs_limit,
    }))
}

pub(crate) async fn get_prompt_class_detail_main(
    State(state): State<AppState>,
    Query(params): Query<PromptClassDetailParams>,
) -> Result<Json<PromptClassDetailDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    prompt_class_detail_response(&state, "main", None, params).await
}

pub(crate) async fn get_prompt_class_detail_subagent(
    State(state): State<AppState>,
    Path(persona): Path<String>,
    Query(params): Query<PromptClassDetailParams>,
) -> Result<Json<PromptClassDetailDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let persona_arg = if persona == SUBAGENT_PERSONA_UNRESOLVED_SEGMENT {
        None
    } else {
        Some(persona.as_str())
    };
    prompt_class_detail_response(&state, "subagent", persona_arg, params).await
}
