use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

#[derive(Debug, Serialize)]
pub(crate) struct AgentDto {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) definition_path: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) source: &'static str,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentAliasDto {
    pub(crate) alias: String,
    pub(crate) kind: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentListItemDto {
    #[serde(flatten)]
    pub(crate) agent: AgentDto,
    pub(crate) composition_count: i64,
    pub(crate) dispatch_count: i64,
    pub(crate) instance_count: i64,
    pub(crate) prompt_version_count: i64,
    pub(crate) last_dispatched_at: Option<String>,
    pub(crate) aliases: Vec<AgentAliasDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentDispatchRowDto {
    pub(crate) tool_use_id: i64,
    pub(crate) dispatched_at: String,
    pub(crate) prompt: String,
    pub(crate) session_uuid: String,
    pub(crate) request_id: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentDispatchesPageDto {
    pub(crate) total_count: i64,
    pub(crate) rows: Vec<AgentDispatchRowDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentPromptVersionDto {
    pub(crate) content_sha256: String,
    pub(crate) first_seen_at: String,
    pub(crate) last_seen_at: String,
    pub(crate) request_count: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentInstanceDto {
    pub(crate) id: i64,
    pub(crate) session_uuid: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) first_request_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentDetailDto {
    #[serde(flatten)]
    pub(crate) agent: AgentDto,
    pub(crate) composition_count: i64,
    pub(crate) dispatch_count: i64,
    pub(crate) instance_count: i64,
    pub(crate) prompt_version_count: i64,
    pub(crate) last_dispatched_at: Option<String>,
    pub(crate) aliases: Vec<AgentAliasDto>,
    pub(crate) dispatches: AgentDispatchesPageDto,
    pub(crate) prompt_versions: Vec<AgentPromptVersionDto>,
    pub(crate) instances: Vec<AgentInstanceDto>,
    pub(crate) compositions: Vec<PromptCompositionDto>,
}

pub(crate) fn agent_to_dto(agent: &crate::agents::Agent) -> AgentDto {
    AgentDto {
        id: agent.id,
        name: agent.name.clone(),
        definition_path: agent.definition_path.clone(),
        description: agent.description.clone(),
        source: agent.source.as_str(),
        created_at: agent.created_at.to_rfc3339(),
        updated_at: agent.updated_at.to_rfc3339(),
    }
}

pub(crate) fn aliases_for_agent(
    all_aliases: &[crate::store::persistence::agents::AgentAlias],
    agent_id: i64,
) -> Vec<AgentAliasDto> {
    all_aliases
        .iter()
        .filter(|a| a.agent_id == agent_id)
        .map(|a| AgentAliasDto {
            alias: a.alias.clone(),
            kind: a.kind.clone(),
        })
        .collect()
}

pub(crate) async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentListItemDto>>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Ok(Json(Vec::new()));
    };
    let rows = handle
        .list_agents_with_evidence()
        .await
        .map_err(map_agent_persist_error)?;
    let all_aliases = handle
        .fetch_alias_map()
        .await
        .map_err(map_agent_persist_error)?;
    let items: Vec<AgentListItemDto> = rows
        .into_iter()
        .map(|row| AgentListItemDto {
            aliases: aliases_for_agent(&all_aliases, row.agent.id),
            agent: agent_to_dto(&row.agent),
            composition_count: row.composition_count,
            dispatch_count: row.dispatch_count,
            instance_count: row.instance_count,
            prompt_version_count: row.prompt_version_count,
            last_dispatched_at: row.last_dispatched_at.map(|ts| ts.to_rfc3339()),
        })
        .collect();
    Ok(Json(items))
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct AgentPatch {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub(crate) definition_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub(crate) description: Option<Option<String>>,
    #[serde(default)]
    pub(crate) source: Option<String>,
}

pub(crate) async fn patch_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AgentPatch>,
) -> Result<Json<AgentDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; agent registry is not available",
        ));
    };
    if body.name.is_none()
        && body.definition_path.is_none()
        && body.description.is_none()
        && body.source.is_none()
    {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "patch body must include at least one of `name`, `definition_path`, `description`, `source`",
        ));
    }
    if let Some(label) = body.name.as_deref()
        && label.is_empty()
    {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            "name must not be empty",
        ));
    }
    let parsed_source = match body.source.as_deref() {
        None => None,
        Some(raw) => Some(
            crate::agents::AgentSource::from_str(raw)
                .map_err(|err| composition_error_response(StatusCode::BAD_REQUEST, &err))?,
        ),
    };
    let Some(existing) = resolve_agent_by_id_or_name(handle, &id).await? else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no agent with that id",
        ));
    };
    let definition_path_patch: Option<Option<&str>> =
        body.definition_path.as_ref().map(|inner| inner.as_deref());
    let description_patch: Option<Option<&str>> =
        body.description.as_ref().map(|inner| inner.as_deref());
    let Some(agent) = handle
        .patch_agent(
            existing.id,
            crate::agents::store::PatchAgentParams {
                name: body.name.as_deref(),
                definition_path: definition_path_patch,
                description: description_patch,
                source: parsed_source,
            },
        )
        .await
        .map_err(map_agent_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no agent with that id",
        ));
    };
    Ok(Json(agent_to_dto(&agent)))
}

pub(crate) async fn delete_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; agent registry is not available",
        ));
    };
    let Some(existing) = resolve_agent_by_id_or_name(handle, &id).await? else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no agent with that id",
        ));
    };
    let removed = handle
        .delete_agent(existing.id)
        .await
        .map_err(map_agent_persist_error)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no agent with that id",
        ))
    }
}

pub(crate) const AGENT_DISPATCHES_DETAIL_DEFAULT_LIMIT: i64 = 50;
pub(crate) const AGENT_DISPATCHES_DETAIL_MAX_LIMIT: i64 = 500;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct GetAgentQueryParams {
    #[serde(default)]
    pub(crate) dispatch_limit: Option<i64>,
    #[serde(default)]
    pub(crate) dispatch_offset: Option<i64>,
}

pub(crate) async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GetAgentQueryParams>,
) -> Result<Json<AgentDetailDto>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; agent registry is not available",
        ));
    };
    let Some(agent) = resolve_agent_by_id_or_name(handle, &id).await? else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no agent with that id",
        ));
    };
    let Some(evidence) = handle
        .fetch_agent_evidence_by_id(agent.id)
        .await
        .map_err(map_agent_persist_error)?
    else {
        return Err(composition_error_response(
            StatusCode::NOT_FOUND,
            "no agent with that id",
        ));
    };
    let all_aliases = handle
        .fetch_alias_map()
        .await
        .map_err(map_agent_persist_error)?;

    let dispatches = fetch_agent_dispatches_page(handle, agent.id, &params).await?;
    let prompt_versions = fetch_agent_prompt_versions(handle, agent.id).await?;
    let instances = fetch_agent_instances(handle, agent.id).await?;
    let compositions = fetch_agent_compositions_with_usage(handle, agent.id).await?;

    Ok(Json(AgentDetailDto {
        aliases: aliases_for_agent(&all_aliases, agent.id),
        agent: agent_to_dto(&agent),
        composition_count: evidence.composition_count,
        dispatch_count: evidence.dispatch_count,
        instance_count: evidence.instance_count,
        prompt_version_count: evidence.prompt_version_count,
        last_dispatched_at: evidence.last_dispatched_at.map(|ts| ts.to_rfc3339()),
        dispatches,
        prompt_versions,
        instances,
        compositions,
    }))
}

type AgentDetailError = (StatusCode, Json<PromptCompositionErrorBody>);

async fn fetch_agent_dispatches_page(
    handle: &crate::store::persistence::PersistHandle,
    agent_id: i64,
    params: &GetAgentQueryParams,
) -> Result<AgentDispatchesPageDto, AgentDetailError> {
    let dispatch_limit = params
        .dispatch_limit
        .unwrap_or(AGENT_DISPATCHES_DETAIL_DEFAULT_LIMIT)
        .clamp(1, AGENT_DISPATCHES_DETAIL_MAX_LIMIT);
    let dispatch_offset = params.dispatch_offset.unwrap_or(0).max(0);
    let dispatch_total = handle
        .count_dispatches_for_agent(agent_id)
        .await
        .map_err(map_agent_persist_error)?;
    let dispatch_rows = handle
        .list_dispatches_for_agent(agent_id, dispatch_limit, dispatch_offset)
        .await
        .map_err(map_agent_persist_error)?;
    Ok(AgentDispatchesPageDto {
        total_count: dispatch_total,
        rows: dispatch_rows
            .into_iter()
            .map(|r| AgentDispatchRowDto {
                tool_use_id: r.tool_use_id,
                dispatched_at: r.dispatched_at.to_rfc3339(),
                prompt: r.prompt,
                session_uuid: r.session_uuid.to_string(),
                request_id: r.request_id,
            })
            .collect(),
    })
}

async fn fetch_agent_prompt_versions(
    handle: &crate::store::persistence::PersistHandle,
    agent_id: i64,
) -> Result<Vec<AgentPromptVersionDto>, AgentDetailError> {
    Ok(handle
        .list_prompt_versions_for_agent(agent_id)
        .await
        .map_err(map_agent_persist_error)?
        .into_iter()
        .map(|r| AgentPromptVersionDto {
            content_sha256: r.content_sha256,
            first_seen_at: r.first_seen_at.to_rfc3339(),
            last_seen_at: r.last_seen_at.to_rfc3339(),
            request_count: r.request_count,
        })
        .collect())
}

async fn fetch_agent_instances(
    handle: &crate::store::persistence::PersistHandle,
    agent_id: i64,
) -> Result<Vec<AgentInstanceDto>, AgentDetailError> {
    Ok(handle
        .list_instances_for_agent(agent_id)
        .await
        .map_err(map_agent_persist_error)?
        .into_iter()
        .map(|r| AgentInstanceDto {
            id: r.id,
            session_uuid: r.session_uuid.to_string(),
            subagent_persona: r.subagent_persona,
            started_at: r.started_at.map(|ts| ts.to_rfc3339()),
            first_request_id: r.first_request_id,
        })
        .collect())
}

async fn fetch_agent_compositions_with_usage(
    handle: &crate::store::persistence::PersistHandle,
    agent_id: i64,
) -> Result<Vec<PromptCompositionDto>, AgentDetailError> {
    let compositions = handle
        .proxy()
        .list_compositions_for_agent(agent_id)
        .await
        .map_err(map_agent_persist_error)?;
    let applied_counts = handle
        .count_applied_24h_all()
        .await
        .map_err(map_agent_persist_error)?;
    Ok(compositions
        .iter()
        .map(|c| composition_to_dto(c, applied_counts.get(&c.id).copied().unwrap_or(0)))
        .collect())
}
