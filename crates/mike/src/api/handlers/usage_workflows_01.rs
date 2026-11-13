use super::*;
use crate::store::persistence::eval_repo::EvalPersist as _;

#[derive(Debug, Serialize)]
pub(crate) struct ModelUsageEntry {
    pub(crate) model: Option<String>,
    pub(crate) requests: i64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cache_creation_tokens: i64,
    pub(crate) cache_read_tokens: i64,
    pub(crate) last_seen: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelUsageResponse {
    pub(crate) days: i32,
    pub(crate) models: Vec<ModelUsageEntry>,
}

pub(crate) async fn get_model_usage(
    State(state): State<AppState>,
    Query(params): Query<ModelUsageQuery>,
) -> Result<Json<ModelUsageResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let days = params.days.unwrap_or(30).clamp(1, 365);
    let rows = handle
        .list_model_usage(days)
        .await
        .map_err(persist_error_response)?;
    let models = rows
        .into_iter()
        .map(|r| ModelUsageEntry {
            model: r.model,
            requests: r.requests,
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            cache_creation_tokens: r.cache_creation_tokens,
            cache_read_tokens: r.cache_read_tokens,
            last_seen: r.last_seen,
        })
        .collect();
    Ok(Json(ModelUsageResponse { days, models }))
}

pub(crate) async fn get_openrouter_usage(
    State(state): State<AppState>,
) -> Result<Json<crate::api::usage::OpenRouterUsageResponse>, (StatusCode, Json<serde_json::Value>)>
{
    use secrecy::ExposeSecret;

    let api_key = state.openrouter_key().map(|s| s.expose_secret().to_owned());

    match state
        .openrouter_usage_cache()
        .get(
            &state.client,
            &state.config.steerer.openrouter_base_url,
            api_key.as_deref(),
            state.persist(),
        )
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::warn!(error = %e, "get_openrouter_usage: OpenRouter fetch failed");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "upstream_error",
                    "message": e.to_string()
                })),
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowAgentDto {
    pub(crate) agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_type: Option<String>,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<String>,
    pub(crate) gaps_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_json: Option<serde_json::Value>,
    pub(crate) request_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowRunDto {
    pub(crate) run_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) phases: Vec<String>,
    pub(crate) parent_request_id: i64,
    pub(crate) agent_count: usize,
    pub(crate) result_count: usize,
    pub(crate) agents: Vec<WorkflowAgentDto>,
}

pub(crate) fn parse_workflow_name(script: &str) -> Option<String> {
    for prefix in ["name:", "\"name\":"] {
        if let Some(pos) = script.find(prefix) {
            let after = script[pos + prefix.len()..].trim_start();
            let quote = after.chars().next()?;
            if quote == '\'' || quote == '"' {
                let inner = &after[1..];
                if let Some(end) = inner.find(quote) {
                    let name = inner[..end].trim().to_string();
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn parse_workflow_description(script: &str) -> Option<String> {
    for prefix in ["description:", "\"description\":"] {
        if let Some(pos) = script.find(prefix) {
            let after = script[pos + prefix.len()..].trim_start();
            let quote = after.chars().next()?;
            if quote == '\'' || quote == '"' {
                let inner = &after[1..];
                if let Some(end) = inner.find(quote) {
                    let desc = inner[..end].trim().to_string();
                    if !desc.is_empty() {
                        return Some(desc);
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn parse_workflow_phases(script: &str) -> Vec<String> {
    let mut phases = Vec::new();
    let Some(start) = script.find("phases:") else {
        return phases;
    };
    let phases_text = &script[start..];
    let mut search = phases_text;
    while let Some(pos) = search.find("title:") {
        let after = search[pos + "title:".len()..].trim_start();
        let Some(quote) = after.chars().next() else {
            break;
        };
        if quote == '\'' || quote == '"' {
            let inner = &after[1..];
            if let Some(end) = inner.find(quote) {
                let title = inner[..end].trim().to_string();
                if !title.is_empty() {
                    phases.push(title);
                }
                search = &inner[end + 1..];
                continue;
            }
        }
        search = &search[pos + "title:".len()..];
    }
    phases
}

pub(crate) fn parse_run_uid(result_text: &str) -> Option<String> {
    let marker = "Run ID: ";
    let pos = result_text.find(marker)?;
    let after = &result_text[pos + marker.len()..];
    let end = after
        .find(|c: char| c.is_whitespace())
        .unwrap_or(after.len());
    let uid = after[..end].trim().to_string();
    if uid.is_empty() { None } else { Some(uid) }
}

async fn fetch_workflow_run_data(
    handle: &crate::store::persistence::PersistHandle,
    session_db_id: i64,
    session_id: &str,
) -> Result<
    (
        Vec<crate::store::persistence::rehydrate::WorkflowRunRow>,
        Vec<crate::store::persistence::rehydrate::WorkflowAgentRow>,
    ),
    Response,
> {
    let runs_rows = handle
        .eval()
        .fetch_workflow_runs(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_id,
                "get_session_workflow_runs: fetch_workflow_runs failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;

    let agent_rows = handle
        .eval()
        .fetch_workflow_agents(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_id,
                "get_session_workflow_runs: fetch_workflow_agents failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;

    Ok((runs_rows, agent_rows))
}

fn build_workflow_agent_dtos(
    window_rows: &[&crate::store::persistence::rehydrate::WorkflowAgentRow],
) -> Vec<WorkflowAgentDto> {
    let mut agent_map: BTreeMap<
        String,
        Vec<&crate::store::persistence::rehydrate::WorkflowAgentRow>,
    > = BTreeMap::new();
    for row in window_rows {
        agent_map.entry(row.agent_id.clone()).or_default().push(row);
    }

    agent_map
        .into_iter()
        .map(|(agent_id, rows)| {
            let result_json = rows.iter().rev().find_map(|r| r.result_json.clone());
            let status = if result_json.is_some() {
                "result".to_string()
            } else {
                "started".to_string()
            };
            let summary = result_json
                .as_ref()
                .and_then(|j| j.get("summary"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let gaps_count: i64 = result_json
                .as_ref()
                .and_then(|j| j.get("gaps"))
                .and_then(|v| v.as_array())
                .map(|a| a.len() as i64)
                .unwrap_or(0);
            let agent_type = rows.iter().find_map(|r| r.subagent_persona.clone());
            let request_ids: Vec<i64> = rows.iter().map(|r| r.request_id).collect();

            WorkflowAgentDto {
                agent_id,
                agent_type,
                status,
                summary,
                gaps_count,
                result_json,
                request_ids,
            }
        })
        .collect()
}

fn build_workflow_run_dto(
    i: usize,
    run: &crate::store::persistence::rehydrate::WorkflowRunRow,
    runs_rows: &[crate::store::persistence::rehydrate::WorkflowRunRow],
    agent_rows: &[crate::store::persistence::rehydrate::WorkflowAgentRow],
) -> WorkflowRunDto {
    let window_start = run.parent_created_at;
    let window_end = runs_rows.get(i + 1).map(|next| next.parent_created_at);

    let window_rows: Vec<&crate::store::persistence::rehydrate::WorkflowAgentRow> = agent_rows
        .iter()
        .filter(|ar| {
            ar.req_created_at >= window_start
                && window_end
                    .map(|end| ar.req_created_at < end)
                    .unwrap_or(true)
        })
        .collect();

    let agents = build_workflow_agent_dtos(&window_rows);
    let agent_count = agents.len();
    let result_count = agents.iter().filter(|a| a.result_json.is_some()).count();

    let (name, description, phases) = if let Some(script) = &run.script {
        (
            parse_workflow_name(script),
            parse_workflow_description(script),
            parse_workflow_phases(script),
        )
    } else {
        (None, None, vec![])
    };
    let run_uid = run.result_text.as_deref().and_then(parse_run_uid);

    WorkflowRunDto {
        run_id: run.use_id,
        run_uid,
        name,
        description,
        phases,
        parent_request_id: run.parent_request_id,
        agent_count,
        result_count,
        agents,
    }
}

pub(crate) async fn get_session_workflow_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<WorkflowRunDto>>, Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session_db_id = handle
        .resolve_session_db_id(&id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %id,
                "get_session_workflow_runs: session_db_id resolve failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;

    let (runs_rows, agent_rows) = fetch_workflow_run_data(handle, session_db_id, &id).await?;

    let dtos: Vec<WorkflowRunDto> = runs_rows
        .iter()
        .enumerate()
        .map(|(i, run)| build_workflow_run_dto(i, run, &runs_rows, &agent_rows))
        .collect();

    Ok(Json(dtos))
}

pub(crate) async fn get_sync_status(
    State(state): State<AppState>,
) -> Json<crate::resync::ResyncStatus> {
    Json(crate::resync::ResyncStatus::clone(
        &state.resync_status().load(),
    ))
}
