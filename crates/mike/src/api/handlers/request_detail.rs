use super::*;

#[derive(Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub(crate) struct RequestDetail {
    pub(crate) id: u64,
    pub(crate) upstream_request_id: Option<String>,
    pub(crate) upstream_message_id: Option<String>,
    pub(crate) session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_number: Option<u64>,

    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) request_headers: Vec<(String, String)>,
    pub(crate) request_body_size: u64,
    pub(crate) model: Option<String>,
    pub(crate) is_streaming: bool,
    pub(crate) message_count: usize,
    pub(crate) is_tool_loop: bool,
    pub(crate) last_user_message: String,
    pub(crate) tool_names: Vec<String>,
    pub(crate) params: RequestParams,
    pub(crate) project: Option<String>,
    pub(crate) git_repo: Option<String>,
    pub(crate) human_prompt: String,
    pub(crate) turn_type: String,
    pub(crate) turn_origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subagent_persona: Option<String>,

    pub(crate) status: RequestStatus,
    pub(crate) response_status: Option<u16>,
    pub(crate) response_headers: Option<Vec<(String, String)>>,
    pub(crate) response_body_size: u64,

    pub(crate) tool_calls: Vec<ToolCall>,
    pub(crate) token_usage: Option<TokenUsage>,
    pub(crate) stop_reason: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub(crate) stop_details: Option<serde_json::Value>,

    pub(crate) created_at: String,
    pub(crate) completed_at: Option<String>,

    pub(crate) diagnostics: Vec<DiagnosticDto>,

    pub(crate) composition_applied: bool,
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub(crate) unknown_blocks: Option<serde_json::Value>,
    pub(crate) system_blocks: Option<SystemBlocksProjection>,
    pub(crate) system_prompt_source: SystemPromptSource,
    pub(crate) filter_modified: bool,
    pub(crate) firing_rule_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) predecessor_request_id: Option<i64>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub(crate) struct SystemBlocksProjection {
    pub(crate) prompt_sha256: Option<String>,
    pub(crate) composition_id: Option<String>,
    pub(crate) composition_version: Option<u64>,
    pub(crate) blocks: Vec<SystemBlockProjection>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub(crate) struct SystemBlockProjection {
    pub(crate) index: u32,
    pub(crate) text: String,
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub(crate) cache_control: Option<serde_json::Value>,
    pub(crate) composition_match: Option<String>,
}

pub(crate) fn build_system_blocks_projection(
    req: &InterceptedRequest,
    composer_cache: &std::collections::HashMap<
        crate::compose_prompt::PromptClassKey,
        crate::wire::composition::Composition,
    >,
) -> Option<SystemBlocksProjection> {
    let arr = req.system_prompt.blocks.as_ref()?.as_array()?;

    let class_key = crate::compose_prompt::PromptClassKey::from_turn_origin_str(
        &req.turn_origin,
        req.subagent_persona.as_deref(),
    );
    let composition: Option<&crate::wire::composition::Composition> =
        class_key.and_then(|key| composer_cache.get(&key));

    let composition_version: Option<u64> =
        composition.map(|c| u64::try_from(c.version.max(0)).unwrap_or(0));

    let blocks = arr
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

            let composition_match: Option<String> =
                composition.map(|comp| match comp.blocks.get(i) {
                    None => "extra".to_string(),
                    Some(comp_block) => {
                        let comp_text = comp_block
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if comp_text == text.as_str() {
                            "matched".to_string()
                        } else {
                            "unknown".to_string()
                        }
                    }
                });

            SystemBlockProjection {
                index,
                text,
                cache_control,
                composition_match,
            }
        })
        .collect();
    Some(SystemBlocksProjection {
        prompt_sha256: req.system_prompt.prompt_sha256.clone(),
        composition_id: req.system_prompt.composition_id.map(|u| u.to_string()),
        composition_version,
        blocks,
    })
}

impl RequestDetail {
    pub(crate) fn from_request(
        req: &InterceptedRequest,
        composer_cache: &std::collections::HashMap<
            crate::compose_prompt::PromptClassKey,
            crate::wire::composition::Composition,
        >,
        predecessor_request_id: Option<i64>,
    ) -> Self {
        Self {
            id: req.id.as_u64(),
            upstream_request_id: req.upstream_request_id.clone(),
            upstream_message_id: req.upstream_message_id.clone(),
            session_id: req.session_id.clone(),
            request_number: None,

            method: req.method.clone(),
            path: req.path.clone(),
            request_headers: req.request_headers.clone(),
            request_body_size: byte_size(req.request_body.as_ref()),
            model: req.model.clone(),
            is_streaming: req.is_streaming,
            message_count: req.message_count,
            is_tool_loop: req.is_tool_loop,
            last_user_message: req.last_user_message.clone(),
            tool_names: req.tool_names.clone(),
            params: req.params.clone(),
            project: req.project.clone(),
            git_repo: req.git_repo.clone(),
            human_prompt: req.human_prompt.clone(),
            turn_type: req.turn_type.clone(),
            turn_origin: req.turn_origin.clone(),
            subagent_persona: req.subagent_persona.clone(),

            status: req.status.clone(),
            response_status: req.response_status,
            response_headers: req.response_headers.clone(),
            response_body_size: byte_size(req.response_body.as_ref()),

            tool_calls: req.tool_calls.clone(),
            token_usage: req.token_usage.clone(),
            stop_reason: req.stop_reason.clone(),
            stop_details: req.stop_details.clone(),

            created_at: req.created_at.to_rfc3339(),
            completed_at: req.completed_at.map(|t| t.to_rfc3339()),

            diagnostics: req.diagnostics.iter().map(DiagnosticDto::from).collect(),

            composition_applied: req.system_prompt.composition_applied,
            unknown_blocks: req.system_prompt.unknown_blocks.clone(),
            system_blocks: build_system_blocks_projection(req, composer_cache),
            system_prompt_source: req.system_prompt.source,
            filter_modified: req.system_prompt.filter_modified,
            firing_rule_ids: req
                .system_prompt
                .firing_rule_ids
                .iter()
                .map(|id| id.to_string())
                .collect(),
            predecessor_request_id,
        }
    }
}

pub(crate) async fn get_request(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestDetail>, StatusCode> {
    let request_id = RequestId::from_raw(id);
    let in_memory = {
        let store = state.store.read().await;
        store.get(&request_id).cloned()
    };
    let req = match in_memory {
        Some(req) => req,
        None => match load_request_from_db(&state, id).await? {
            Some(req) => req,
            None => return Err(StatusCode::NOT_FOUND),
        },
    };
    let predecessor_request_id = if req.turn_origin == "primary" || req.turn_origin == "subagent" {
        match state.persist() {
            Some(handle) => {
                let session_db_id = handle
                    .resolve_session_db_id(&req.session_id)
                    .await
                    .map_err(|e| {
                        tracing::warn!(
                            error = %e,
                            request_id = id,
                            "get_request: session_db_id resolution failed",
                        );
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                match session_db_id {
                    Some(db_id) => handle
                        .fetch_predecessor_request_id(db_id, req.created_at, req.agent_instance_id)
                        .await
                        .map_err(|e| {
                            tracing::warn!(
                                error = %e,
                                request_id = id,
                                "get_request: predecessor lookup failed",
                            );
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?,
                    None => None,
                }
            }
            None => None,
        }
    } else {
        None
    };
    let request_number = match state.persist() {
        Some(handle) => handle
            .fetch_request_number(id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    request_id = id,
                    "get_request: request_number lookup failed",
                );
                None
            })
            .map(|n| n.max(0) as u64),
        None => None,
    };
    let composer_cache = state.composer.cache().load();
    let mut detail = RequestDetail::from_request(&req, &composer_cache, predecessor_request_id);
    detail.request_number = request_number;
    Ok(Json(detail))
}

pub(crate) async fn load_request_from_db(
    state: &AppState,
    id: u64,
) -> Result<Option<InterceptedRequest>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Ok(None);
    };
    match handle.fetch_request_by_id(id).await {
        Ok(opt) => Ok(opt),
        Err(e) => {
            tracing::warn!(
                error = %e,
                request_id = id,
                "request detail DB fallback failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
