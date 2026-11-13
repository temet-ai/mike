use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

#[derive(Debug, Clone, Serialize)]
pub struct PromptClassVersionModelDto {
    pub model: Option<String>,
    pub observation_count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub latest_request_id: String,
    pub latest_session_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptClassVersionDto {
    pub version: Option<String>,
    pub builds: i64,
    pub observation_count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub models: Vec<PromptClassVersionModelDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptClassVersionsDto {
    pub lane: String,
    pub subagent_persona: Option<String>,
    pub versions: Vec<PromptClassVersionDto>,
}

fn group_prompt_class_versions(
    rows: Vec<crate::store::persistence::compositions::PromptClassVersionRow>,
) -> Vec<PromptClassVersionDto> {
    let mut versions: Vec<PromptClassVersionDto> = Vec::new();
    for row in rows {
        let model = PromptClassVersionModelDto {
            model: row.model,
            observation_count: row.model_observation_count,
            first_seen: row.model_first_seen.to_rfc3339(),
            last_seen: row.model_last_seen.to_rfc3339(),
            latest_request_id: row.latest_request_id.to_string(),
            latest_session_id: row.latest_session_id.to_string(),
        };
        match versions.last_mut() {
            Some(current) if current.version == row.release => current.models.push(model),
            _ => versions.push(PromptClassVersionDto {
                version: row.release,
                builds: row.builds,
                observation_count: row.release_observation_count,
                first_seen: row.release_first_seen.to_rfc3339(),
                last_seen: row.release_last_seen.to_rfc3339(),
                models: vec![model],
            }),
        }
    }
    versions
}

async fn prompt_class_versions_response(
    state: &AppState,
    lane: &str,
    persona: Option<&str>,
) -> Result<Json<PromptClassVersionsDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; prompt-class versions are not available",
        ));
    };
    let Some(rows) = handle
        .proxy()
        .get_prompt_class_versions(lane, persona)
        .await
        .map_err(map_composition_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no prompt class observed or registered for that (lane, persona)",
        ));
    };
    Ok(Json(PromptClassVersionsDto {
        lane: lane.to_string(),
        subagent_persona: persona.map(str::to_string),
        versions: group_prompt_class_versions(rows),
    }))
}

pub async fn get_prompt_class_versions_main(
    State(state): State<AppState>,
) -> Result<Json<PromptClassVersionsDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    prompt_class_versions_response(&state, "main", None).await
}

pub async fn get_prompt_class_versions_subagent(
    State(state): State<AppState>,
    Path(persona): Path<String>,
) -> Result<Json<PromptClassVersionsDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let persona_arg = if persona == SUBAGENT_PERSONA_UNRESOLVED_SEGMENT {
        None
    } else {
        Some(persona.as_str())
    };
    prompt_class_versions_response(&state, "subagent", persona_arg).await
}
