use super::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptCompositionBlockDto {
    pub index: u32,
    pub text: String,
    pub cache_control: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct PromptCompositionDto {
    pub id: String,
    pub prompt_sha256: String,
    pub label: String,
    pub version: i64,
    pub blocks: Vec<PromptCompositionBlockDto>,
    pub mode: String,
    pub notes: Option<String>,
    pub agent_name: Option<String>,
    pub agent_definition_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub applied_count_24h: i64,
}

#[derive(Debug, Serialize)]
pub struct PromptCompositionListItemDto {
    pub id: String,
    pub prompt_sha256: String,
    pub label: String,
    pub version: i64,
    pub mode: String,
    pub block_count: u32,
    pub total_chars: u32,
    pub applied_count_24h: i64,
    pub updated_at: String,
    pub agent_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptCompositionCreate {
    pub prompt_sha256: String,
    pub label: String,
    pub blocks: Vec<PromptCompositionBlockInput>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub agent_definition_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PromptCompositionBlockInput {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PromptCompositionPatch {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub blocks: Option<Vec<PromptCompositionBlockInput>>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub agent_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub agent_definition_path: Option<Option<String>>,
}

pub fn deserialize_optional_nullable_string<'de, D>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let inner = Option::<String>::deserialize(deserializer)?;
    Ok(Some(inner))
}

#[derive(Debug, Serialize)]
pub struct PromptCompositionMutationEnvelope {
    pub composition: PromptCompositionDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PromptCompositionDeleteWarnings {
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PromptCompositionErrorBody {
    pub error: String,
}

pub fn composition_error_response(
    status: StatusCode,
    error: &str,
) -> (StatusCode, Json<PromptCompositionErrorBody>) {
    (
        status,
        Json(PromptCompositionErrorBody {
            error: error.to_string(),
        }),
    )
}

pub fn parse_composition_id(
    raw: &str,
) -> Result<i64, (StatusCode, Json<PromptCompositionErrorBody>)> {
    raw.parse::<i64>().map_err(|_| {
        composition_error_response(StatusCode::NOT_FOUND, "no composition with that id")
    })
}

pub fn is_valid_prompt_sha256(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn parse_prompt_sha256_path(
    raw: &str,
) -> Result<&str, (StatusCode, Json<PromptCompositionErrorBody>)> {
    if is_valid_prompt_sha256(raw) {
        Ok(raw)
    } else {
        Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "prompt_sha256 must be 64 lowercase hex characters",
        ))
    }
}

pub fn parse_mode_input(
    raw: &str,
) -> Result<crate::wire::composition::CompositionMode, (StatusCode, Json<PromptCompositionErrorBody>)>
{
    crate::wire::composition::CompositionMode::from_str(raw).map_err(|_| {
        composition_error_response(
            StatusCode::BAD_REQUEST,
            "mode must be one of \"observe\", \"template\", or \"replace\"",
        )
    })
}

pub fn parse_supported_mode_input(
    raw: &str,
) -> Result<crate::wire::composition::CompositionMode, (StatusCode, Json<PromptCompositionErrorBody>)>
{
    use crate::wire::composition::CompositionMode;
    let parsed = parse_mode_input(raw)?;
    match parsed {
        CompositionMode::Replace => Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "replace mode is not supported in V1; choose observe or template",
        )),
        other => Ok(other),
    }
}

pub fn reject_subagent_alteration(
    mode: crate::wire::composition::CompositionMode,
    lane: &str,
) -> Result<(), (StatusCode, Json<PromptCompositionErrorBody>)> {
    use crate::wire::composition::CompositionMode;
    if lane == "subagent" && mode != CompositionMode::Observe {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "subagent-lane compositions (agent_name set) only support observe mode; \
             the composer never alters a subagent's system prompt",
        ));
    }
    Ok(())
}

pub fn map_composition_persist_error(
    err: crate::store::persistence::PersistError,
) -> (StatusCode, Json<PromptCompositionErrorBody>) {
    use crate::store::persistence::PersistError;
    if let PersistError::Database(ref sqlx_err) = err
        && let Some(db_err) = sqlx_err.as_database_error()
        && db_err.is_unique_violation()
    {
        return composition_error_response(
            StatusCode::CONFLICT,
            "a composition is already registered for that prompt class \
                     (one per subagent agent_name, one for the main lane)",
        );
    }
    tracing::error!(error = %err, "prompt composition persistence failure");
    composition_error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "database error while processing composition",
    )
}

pub const MAX_AGENT_PATH_SEGMENT_LEN: usize = 256;

pub async fn resolve_agent_by_id_or_name(
    handle: &crate::store::persistence::PersistHandle,
    raw: &str,
) -> Result<Option<crate::agents::Agent>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    if raw.is_empty() {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "agent id must not be empty",
        ));
    }
    if raw.len() > MAX_AGENT_PATH_SEGMENT_LEN {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "agent id is too long",
        ));
    }
    if let Ok(id) = raw.parse::<i64>() {
        return handle
            .fetch_agent_by_id(id)
            .await
            .map_err(map_agent_persist_error);
    }
    handle
        .fetch_agent_by_name(raw)
        .await
        .map_err(map_agent_persist_error)
}

pub fn map_agent_persist_error(
    err: crate::store::persistence::PersistError,
) -> (StatusCode, Json<PromptCompositionErrorBody>) {
    use crate::store::persistence::PersistError;
    if let PersistError::Database(ref sqlx_err) = err
        && let Some(db_err) = sqlx_err.as_database_error()
        && db_err.is_unique_violation()
    {
        return composition_error_response(
            StatusCode::CONFLICT,
            "another agent already uses that name",
        );
    }
    tracing::error!(error = %err, "agent registry persistence failure");
    composition_error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "database error while processing agent",
    )
}

pub fn composition_to_dto(
    composition: &crate::wire::composition::Composition,
    applied_count_24h: i64,
) -> PromptCompositionDto {
    let blocks = composition
        .blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            let text = block
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cache_control = block.get("cache_control").cloned();
            let index = u32::try_from(i).unwrap_or(u32::MAX);
            PromptCompositionBlockDto {
                index,
                text,
                cache_control,
            }
        })
        .collect();
    PromptCompositionDto {
        id: composition.id.to_string(),
        prompt_sha256: composition.prompt_sha256.clone(),
        label: composition.label.clone(),
        version: composition.version,
        blocks,
        mode: composition.mode.as_str().to_string(),
        notes: composition.notes.clone(),
        agent_name: composition.agent_name.clone(),
        agent_definition_path: composition.agent_definition_path.clone(),
        created_at: composition.created_at.to_rfc3339(),
        updated_at: composition.updated_at.to_rfc3339(),
        applied_count_24h,
    }
}

pub fn composition_patch_is_empty(body: &PromptCompositionPatch) -> bool {
    body.label.is_none()
        && body.blocks.is_none()
        && body.mode.is_none()
        && body.notes.is_none()
        && body.agent_name.is_none()
        && body.agent_definition_path.is_none()
}
