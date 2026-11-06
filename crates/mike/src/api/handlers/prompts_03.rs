use super::*;

pub(crate) async fn list_prompt_composition_revisions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<PromptCompositionRevisionDto>>, (StatusCode, Json<PromptCompositionErrorBody>)>
{
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; revisions are not available",
        ));
    };
    let uuid = parse_composition_id(&id)?;
    let exists = handle
        .fetch_composition_by_id(uuid)
        .await
        .map_err(map_composition_persist_error)?;
    if exists.is_none() {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no composition with that id",
        ));
    }
    let revisions = handle
        .list_composition_revisions(uuid)
        .await
        .map_err(map_composition_persist_error)?;
    Ok(Json(revisions.into_iter().map(revision_to_dto).collect()))
}

#[derive(Debug, Serialize)]
pub(crate) struct VolatileLinePatternDto {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pattern: String,
    pub(crate) applies_to_block_index: u8,
    pub(crate) enabled: bool,
    pub(crate) notes: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VolatileLinePatternCreate {
    pub(crate) name: String,
    pub(crate) pattern: String,
    pub(crate) applies_to_block_index: u8,
    #[serde(default)]
    pub(crate) enabled: Option<bool>,
    #[serde(default)]
    pub(crate) notes: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct VolatileLinePatternPatch {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) pattern: Option<String>,
    #[serde(default)]
    pub(crate) applies_to_block_index: Option<u8>,
    #[serde(default)]
    pub(crate) enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub(crate) notes: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct VolatileLinePatternErrorBody {
    pub(crate) error: String,
}

pub(crate) fn volatile_error_response(
    status: StatusCode,
    error: &str,
) -> (StatusCode, Json<VolatileLinePatternErrorBody>) {
    (
        status,
        Json(VolatileLinePatternErrorBody {
            error: error.to_string(),
        }),
    )
}

pub(crate) fn parse_volatile_id(
    raw: &str,
) -> Result<i64, (StatusCode, Json<VolatileLinePatternErrorBody>)> {
    raw.parse::<i64>().map_err(|_| {
        volatile_error_response(
            StatusCode::NOT_FOUND,
            "no volatile-line pattern with that id",
        )
    })
}

pub(crate) fn map_volatile_persist_error(
    err: crate::store::persistence::PersistError,
) -> (StatusCode, Json<VolatileLinePatternErrorBody>) {
    use crate::store::persistence::PersistError;
    if let PersistError::Database(ref sqlx_err) = err
        && let Some(db_err) = sqlx_err.as_database_error()
        && db_err.is_unique_violation()
    {
        return volatile_error_response(
            StatusCode::CONFLICT,
            "a volatile-line pattern with that name already exists",
        );
    }
    tracing::error!(error = %err, "volatile-line pattern persistence failure");
    volatile_error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "database error while processing volatile-line pattern",
    )
}

pub(crate) fn volatile_row_to_dto(
    row: crate::compose_prompt::volatile::VolatilePatternRow,
) -> VolatileLinePatternDto {
    VolatileLinePatternDto {
        id: row.id,
        name: row.name,
        pattern: row.pattern,
        applies_to_block_index: row.applies_to_block_index,
        enabled: row.enabled,
        notes: row.notes,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

pub(crate) const MAX_VOLATILE_PATTERN_LEN: usize = 500;

pub(crate) fn validate_volatile_inputs(
    name: Option<&str>,
    pattern: Option<&str>,
    block_index: Option<u8>,
) -> Result<(), (StatusCode, Json<VolatileLinePatternErrorBody>)> {
    if let Some(name) = name
        && name.is_empty()
    {
        return Err(volatile_error_response(
            StatusCode::BAD_REQUEST,
            "name must not be empty",
        ));
    }
    if let Some(pattern) = pattern {
        if pattern.is_empty() {
            return Err(volatile_error_response(
                StatusCode::BAD_REQUEST,
                "pattern must not be empty",
            ));
        }
        if pattern.len() > MAX_VOLATILE_PATTERN_LEN {
            return Err(volatile_error_response(
                StatusCode::BAD_REQUEST,
                &format!("pattern length exceeds {MAX_VOLATILE_PATTERN_LEN} characters",),
            ));
        }
        if let Err(err) = regex::Regex::new(pattern) {
            return Err(volatile_error_response(
                StatusCode::BAD_REQUEST,
                &format!("pattern is not a valid regex: {err}"),
            ));
        }
    }
    if let Some(idx) = block_index
        && idx > 2
    {
        return Err(volatile_error_response(
            StatusCode::BAD_REQUEST,
            "applies_to_block_index must be 0, 1, or 2",
        ));
    }
    Ok(())
}

pub(crate) async fn refresh_volatile_cache(state: &AppState) {
    let Some(handle) = state.persist() else {
        return;
    };
    let cache = state.composer.volatile_cache();
    if let Err(err) = handle.reload_volatile_cache(&cache).await {
        tracing::warn!(error = %err, "volatile-line cache reload failed after CRUD write");
    }
}

pub(crate) async fn list_volatile_lines(
    State(state): State<AppState>,
) -> Result<Json<Vec<VolatileLinePatternDto>>, (StatusCode, Json<VolatileLinePatternErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Ok(Json(Vec::new()));
    };
    let rows = handle
        .list_volatile_patterns()
        .await
        .map_err(map_volatile_persist_error)?;
    Ok(Json(rows.into_iter().map(volatile_row_to_dto).collect()))
}

pub(crate) async fn get_volatile_line(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<VolatileLinePatternDto>, (StatusCode, Json<VolatileLinePatternErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(volatile_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; pattern lookup is not available",
        ));
    };
    let uuid = parse_volatile_id(&id)?;
    let Some(row) = handle
        .fetch_volatile_pattern_by_id(uuid)
        .await
        .map_err(map_volatile_persist_error)?
    else {
        return Err(volatile_error_response(
            StatusCode::NOT_FOUND,
            "no volatile-line pattern with that id",
        ));
    };
    Ok(Json(volatile_row_to_dto(row)))
}

pub(crate) async fn create_volatile_line(
    State(state): State<AppState>,
    Json(body): Json<VolatileLinePatternCreate>,
) -> Result<
    (StatusCode, Json<VolatileLinePatternDto>),
    (StatusCode, Json<VolatileLinePatternErrorBody>),
> {
    let Some(handle) = state.persist() else {
        return Err(volatile_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; pattern mutation is not available",
        ));
    };
    validate_volatile_inputs(
        Some(&body.name),
        Some(&body.pattern),
        Some(body.applies_to_block_index),
    )?;
    let row = handle
        .create_volatile_pattern(
            &body.name,
            &body.pattern,
            body.applies_to_block_index,
            body.enabled.unwrap_or(true),
            body.notes.as_deref(),
        )
        .await
        .map_err(map_volatile_persist_error)?;
    refresh_volatile_cache(&state).await;
    Ok((StatusCode::CREATED, Json(volatile_row_to_dto(row))))
}

pub(crate) async fn patch_volatile_line(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VolatileLinePatternPatch>,
) -> Result<Json<VolatileLinePatternDto>, (StatusCode, Json<VolatileLinePatternErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(volatile_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; pattern mutation is not available",
        ));
    };
    let uuid = parse_volatile_id(&id)?;
    if body.name.is_none()
        && body.pattern.is_none()
        && body.applies_to_block_index.is_none()
        && body.enabled.is_none()
        && body.notes.is_none()
    {
        return Err(volatile_error_response(
            StatusCode::BAD_REQUEST,
            "patch body must include at least one of `name`, `pattern`, `applies_to_block_index`, `enabled`, or `notes`",
        ));
    }
    validate_volatile_inputs(
        body.name.as_deref(),
        body.pattern.as_deref(),
        body.applies_to_block_index,
    )?;
    let notes_patch: Option<Option<&str>> = body.notes.as_ref().map(|inner| inner.as_deref());
    let Some(row) = handle
        .patch_volatile_pattern(
            uuid,
            crate::store::persistence::volatile::VolatilePatternPatch {
                name: body.name.as_deref(),
                pattern: body.pattern.as_deref(),
                applies_to_block_index: body.applies_to_block_index,
                enabled: body.enabled,
                notes: notes_patch,
            },
        )
        .await
        .map_err(map_volatile_persist_error)?
    else {
        return Err(volatile_error_response(
            StatusCode::NOT_FOUND,
            "no volatile-line pattern with that id",
        ));
    };
    refresh_volatile_cache(&state).await;
    Ok(Json(volatile_row_to_dto(row)))
}

pub(crate) async fn delete_volatile_line(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<VolatileLinePatternErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(volatile_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; pattern mutation is not available",
        ));
    };
    let uuid = parse_volatile_id(&id)?;
    let removed = handle
        .delete_volatile_pattern(uuid)
        .await
        .map_err(map_volatile_persist_error)?;
    if !removed {
        return Err(volatile_error_response(
            StatusCode::NOT_FOUND,
            "no volatile-line pattern with that id",
        ));
    }
    refresh_volatile_cache(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PromptCompositionExportEntry {
    pub(crate) prompt_sha256: String,
    pub(crate) label: String,
    pub(crate) blocks: Vec<PromptCompositionBlockInput>,
    pub(crate) mode: String,
    #[serde(default)]
    pub(crate) notes: Option<String>,
    #[serde(default)]
    pub(crate) agent_name: Option<String>,
    #[serde(default)]
    pub(crate) agent_definition_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PromptCompositionExport {
    pub(crate) version: u32,
    pub(crate) compositions: Vec<PromptCompositionExportEntry>,
}

pub(crate) const EXPORT_VERSION_CURRENT: u32 = 1;

pub(crate) async fn export_prompt_compositions(
    State(state): State<AppState>,
) -> Result<Json<PromptCompositionExport>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Ok(Json(PromptCompositionExport {
            version: EXPORT_VERSION_CURRENT,
            compositions: Vec::new(),
        }));
    };
    let map = handle
        .load_prompt_compositions()
        .await
        .map_err(map_composition_persist_error)?;
    let mut entries: Vec<PromptCompositionExportEntry> = map
        .into_values()
        .map(|c| {
            let blocks = c
                .blocks
                .iter()
                .map(|b| {
                    let text = b
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let cache_control = b.get("cache_control").cloned();
                    PromptCompositionBlockInput {
                        text,
                        cache_control,
                    }
                })
                .collect::<Vec<_>>();
            PromptCompositionExportEntry {
                prompt_sha256: c.prompt_sha256,
                label: c.label,
                blocks,
                mode: c.mode.as_str().to_string(),
                notes: c.notes,
                agent_name: c.agent_name,
                agent_definition_path: c.agent_definition_path,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.prompt_sha256.cmp(&b.prompt_sha256));
    Ok(Json(PromptCompositionExport {
        version: EXPORT_VERSION_CURRENT,
        compositions: entries,
    }))
}
