use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

const INSTRUCTIONS_LIMIT: i64 = 50;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptClassInstructionsDto {
    pub(crate) instructions: Option<String>,
    pub(crate) observation_count: i64,
    pub(crate) last_seen: String,
    pub(crate) latest_request_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PromptClassInstructionsListDto {
    pub(crate) lane: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) prompts: Vec<PromptClassInstructionsDto>,
}

pub(crate) async fn get_prompt_class_instructions_subagent(
    State(state): State<AppState>,
    Path(persona): Path<String>,
) -> Result<Json<PromptClassInstructionsListDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let persona_arg = (persona != SUBAGENT_PERSONA_UNRESOLVED_SEGMENT).then_some(persona.as_str());
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; prompt-class instructions are not available",
        ));
    };
    let rows = handle
        .proxy()
        .list_prompt_class_instructions("subagent", persona_arg, INSTRUCTIONS_LIMIT)
        .await
        .map_err(map_composition_persist_error)?;
    Ok(Json(PromptClassInstructionsListDto {
        lane: "subagent".to_string(),
        subagent_persona: persona_arg.map(str::to_string),
        prompts: rows
            .into_iter()
            .map(|row| PromptClassInstructionsDto {
                instructions: row.instructions,
                observation_count: row.observation_count,
                last_seen: row.last_seen.to_rfc3339(),
                latest_request_id: row.latest_request_id.to_string(),
            })
            .collect(),
    }))
}
