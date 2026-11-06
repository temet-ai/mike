use super::*;

pub(crate) fn validate_blocks_input(
    blocks: &[PromptCompositionBlockInput],
) -> Result<serde_json::Value, (StatusCode, Json<PromptCompositionErrorBody>)> {
    if blocks.is_empty() {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "blocks must not be empty",
        ));
    }
    let normalized: Vec<serde_json::Value> = blocks
        .iter()
        .map(|b| {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::Value::String("text".into()));
            obj.insert(
                "text".to_string(),
                serde_json::Value::String(b.text.clone()),
            );
            if let Some(cc) = b.cache_control.as_ref() {
                obj.insert("cache_control".to_string(), cc.clone());
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    Ok(serde_json::Value::Array(normalized))
}

pub(crate) async fn republish_bundles_after_composition(state: &AppState, composition_key: &str) {
    if let Some(publisher) = state
        .steerer_service()
        .and_then(|service| service.bundle_publisher())
    {
        publisher
            .republish(crate::eval::bundles::BundleCause::composition(
                composition_key,
            ))
            .await;
    }
}

pub(crate) async fn refresh_composer_cache(state: &AppState) {
    let Some(handle) = state.persist() else {
        return;
    };
    match handle.load_prompt_compositions().await {
        Ok(seed) => state.composer.cache().store(seed),
        Err(err) => {
            tracing::warn!(error = %err, "composer cache reload failed after CRUD write");
        }
    }
}

pub(crate) async fn list_prompt_compositions(
    State(state): State<AppState>,
) -> Result<Json<Vec<PromptCompositionListItemDto>>, (StatusCode, Json<PromptCompositionErrorBody>)>
{
    let Some(handle) = state.persist() else {
        return Ok(Json(Vec::new()));
    };
    let map = handle
        .load_prompt_compositions()
        .await
        .map_err(map_composition_persist_error)?;

    let applied_counts = handle
        .count_applied_24h_all()
        .await
        .map_err(map_composition_persist_error)?;

    let mut items: Vec<PromptCompositionListItemDto> = Vec::with_capacity(map.len());
    for composition in map.values() {
        let applied = applied_counts.get(&composition.id).copied().unwrap_or(0);
        let block_count = clamp_u32_with_warn(
            composition.blocks.len(),
            "block_count",
            &composition.prompt_sha256,
        );
        let total_chars_raw: usize = composition
            .blocks
            .iter()
            .map(|b| {
                b.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().count())
                    .unwrap_or(0)
            })
            .sum();
        let total_chars =
            clamp_u32_with_warn(total_chars_raw, "total_chars", &composition.prompt_sha256);
        items.push(PromptCompositionListItemDto {
            id: composition.id.to_string(),
            prompt_sha256: composition.prompt_sha256.clone(),
            label: composition.label.clone(),
            version: composition.version,
            mode: composition.mode.as_str().to_string(),
            block_count,
            total_chars,
            applied_count_24h: applied,
            updated_at: composition.updated_at.to_rfc3339(),
            agent_name: composition.agent_name.clone(),
        });
    }
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(Json(items))
}

pub(crate) fn clamp_u32_with_warn(raw: usize, field: &str, prompt_sha256: &str) -> u32 {
    match u32::try_from(raw) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(
                field,
                value = raw,
                prompt_sha256,
                "composition list field exceeds u32::MAX; clamping to u32::MAX"
            );
            u32::MAX
        }
    }
}

pub(crate) async fn get_prompt_composition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PromptCompositionDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; composition lookup is not available",
        ));
    };
    let uuid = parse_composition_id(&id)?;
    let Some(composition) = handle
        .fetch_composition_by_id(uuid)
        .await
        .map_err(map_composition_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no composition with that id",
        ));
    };
    let applied = handle
        .count_applied_24h(composition.id)
        .await
        .map_err(map_composition_persist_error)?;
    Ok(Json(composition_to_dto(&composition, applied)))
}

pub(crate) async fn get_prompt_composition_by_sha256(
    State(state): State<AppState>,
    Path(prompt_sha256): Path<String>,
) -> Result<Json<PromptCompositionDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let prompt_sha256 = parse_prompt_sha256_path(&prompt_sha256)?.to_string();
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; composition lookup is not available",
        ));
    };
    let Some(composition) = handle
        .fetch_composition_by_prompt_sha256(&prompt_sha256)
        .await
        .map_err(map_composition_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no composition for that prompt_sha256",
        ));
    };
    let applied = handle
        .count_applied_24h(composition.id)
        .await
        .map_err(map_composition_persist_error)?;
    Ok(Json(composition_to_dto(&composition, applied)))
}

pub(crate) async fn create_prompt_composition(
    State(state): State<AppState>,
    Json(body): Json<PromptCompositionCreate>,
) -> Result<
    (StatusCode, Json<PromptCompositionMutationEnvelope>),
    (StatusCode, Json<PromptCompositionErrorBody>),
> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; composition mutation is not available",
        ));
    };
    parse_prompt_sha256_path(&body.prompt_sha256)?;
    if body.label.is_empty() {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "label must not be empty",
        ));
    }
    let blocks_json = validate_blocks_input(&body.blocks)?;
    let mode = match body.mode.as_deref() {
        Some(raw) => Some(parse_supported_mode_input(raw)?),
        None => None,
    };
    if let Some(mode) = mode {
        let lane = match body.agent_name.as_deref() {
            Some(name) if !name.is_empty() => {
                crate::compose_prompt::PromptClassKey::Subagent(name.to_string())
            }
            _ => crate::compose_prompt::PromptClassKey::Main,
        }
        .lane();
        reject_subagent_alteration(mode, lane)?;
    }
    let composition = handle
        .create_composition(
            crate::store::persistence::compositions::NewCompositionParams {
                prompt_sha256: &body.prompt_sha256,
                label: &body.label,
                blocks: &blocks_json,
                mode,
                agent_name: body.agent_name.as_deref(),
                agent_definition_path: body.agent_definition_path.as_deref(),
            },
        )
        .await
        .map_err(map_composition_persist_error)?;
    refresh_composer_cache(&state).await;
    republish_bundles_after_composition(&state, &composition.id.to_string()).await;
    let applied = handle
        .count_applied_24h(composition.id)
        .await
        .map_err(map_composition_persist_error)?;
    Ok((
        StatusCode::CREATED,
        Json(PromptCompositionMutationEnvelope {
            composition: composition_to_dto(&composition, applied),
            warnings: Vec::new(),
        }),
    ))
}

pub(crate) async fn patch_prompt_composition(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PromptCompositionPatch>,
) -> Result<Json<PromptCompositionMutationEnvelope>, (StatusCode, Json<PromptCompositionErrorBody>)>
{
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; composition mutation is not available",
        ));
    };
    let uuid = parse_composition_id(&id)?;
    if composition_patch_is_empty(&body) {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "patch body must include at least one of `label`, `blocks`, `mode`, `notes`, `agent_name`, or `agent_definition_path`",
        ));
    }
    if let Some(label) = body.label.as_deref()
        && label.is_empty()
    {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "label must not be empty",
        ));
    }
    let blocks_json: Option<serde_json::Value> = match body.blocks.as_ref() {
        Some(blocks) => Some(validate_blocks_input(blocks)?),
        None => None,
    };
    let mode_parsed: Option<crate::compose_prompt::decision::CompositionMode> =
        match body.mode.as_deref() {
            Some(raw) => Some(parse_supported_mode_input(raw)?),
            None => None,
        };
    if let Some(mode) = mode_parsed
        && mode != crate::compose_prompt::decision::CompositionMode::Observe
    {
        let Some(existing) = handle
            .fetch_composition_by_id(uuid)
            .await
            .map_err(map_composition_persist_error)?
        else {
            return Err(composition_error_response(
                StatusCode::NOT_FOUND,
                "no composition with that id",
            ));
        };
        reject_subagent_alteration(mode, &existing.lane)?;
    }
    let notes_patch: Option<Option<&str>> = body.notes.as_ref().map(|inner| inner.as_deref());
    let agent_name_patch: Option<Option<&str>> =
        body.agent_name.as_ref().map(|inner| inner.as_deref());
    let agent_definition_path_patch: Option<Option<&str>> = body
        .agent_definition_path
        .as_ref()
        .map(|inner| inner.as_deref());

    let Some(composition) = handle
        .patch_composition(
            uuid,
            crate::compose_prompt::store::PatchCompositionParams {
                label: body.label.as_deref(),
                blocks: blocks_json.as_ref(),
                mode: mode_parsed,
                notes: notes_patch,
                agent_name: agent_name_patch,
                agent_definition_path: agent_definition_path_patch,
            },
        )
        .await
        .map_err(map_composition_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no composition with that id",
        ));
    };
    refresh_composer_cache(&state).await;
    republish_bundles_after_composition(&state, &composition.id.to_string()).await;
    let applied = handle
        .count_applied_24h(composition.id)
        .await
        .map_err(map_composition_persist_error)?;
    Ok(Json(PromptCompositionMutationEnvelope {
        composition: composition_to_dto(&composition, applied),
        warnings: Vec::new(),
    }))
}

pub(crate) async fn delete_prompt_composition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PromptCompositionDeleteWarnings>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; composition mutation is not available",
        ));
    };
    let uuid = parse_composition_id(&id)?;
    let outcome = handle
        .delete_composition(uuid)
        .await
        .map_err(map_composition_persist_error)?;
    use crate::compose_prompt::store::DeleteOutcome;
    match outcome {
        DeleteOutcome::Removed => {
            refresh_composer_cache(&state).await;
            republish_bundles_after_composition(&state, &uuid.to_string()).await;
            Ok(Json(PromptCompositionDeleteWarnings {
                warnings: Vec::new(),
            }))
        }
        DeleteOutcome::NotFound => Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no composition with that id",
        )),
        DeleteOutcome::StillReferenced { count } => Err(composition_error_response(
            StatusCode::CONFLICT,
            &format!(
                "composition has been applied to {count} request{} and cannot be deleted",
                if count == 1 { "" } else { "s" }
            ),
        )),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct PromptCompositionRevisionDto {
    pub(crate) id: String,
    pub(crate) composition_id: String,
    pub(crate) version: i64,
    pub(crate) label: String,
    pub(crate) blocks: Vec<PromptCompositionBlockDto>,
    pub(crate) mode: String,
    pub(crate) notes: Option<String>,
    pub(crate) created_at: String,
}

pub(crate) fn revision_to_dto(
    rev: crate::compose_prompt::store::CompositionRevision,
) -> PromptCompositionRevisionDto {
    let blocks = rev
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
    PromptCompositionRevisionDto {
        id: rev.id.to_string(),
        composition_id: rev.composition_id.to_string(),
        version: rev.version,
        label: rev.label,
        blocks,
        mode: rev.mode.as_str().to_string(),
        notes: rev.notes,
        created_at: rev.created_at.to_rfc3339(),
    }
}
