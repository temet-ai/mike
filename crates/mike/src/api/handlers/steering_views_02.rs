use super::*;

async fn fetch_session_analyzer_overrides(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
) -> Result<std::collections::HashMap<String, bool>, StatusCode> {
    handle
        .get_session_analyzer_overrides(session_uuid)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_analyzers: analyzer-override query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn fetch_session_diagnostic_hit_count_map(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
) -> Result<std::collections::HashMap<String, i64>, StatusCode> {
    handle
        .fetch_session_diagnostic_hit_counts(session_uuid)
        .await
        .map(|rows| rows.into_iter().collect())
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_analyzers: diagnostic hit-count query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn fetch_weasel_intercept_row(
    state: &AppState,
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
    overrides: &std::collections::HashMap<String, bool>,
) -> Result<SessionAnalyzerDto, StatusCode> {
    let weasel_hit_count = handle
        .fetch_intercepted_count_for_session(session_uuid)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_analyzers: weasel-intercept count query failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let weasel_enabled = overrides
        .get("weasel_intercept")
        .copied()
        .unwrap_or(state.feature_toggle_overlay.load().weasel_intercept);
    Ok(SessionAnalyzerDto {
        analyzer_name: "weasel_intercept".to_string(),
        display_name: "Weasel intercept".to_string(),
        enabled: weasel_enabled,
        hit_count: weasel_hit_count,
        is_intercept: true,
    })
}

pub(crate) async fn get_session_analyzers(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<Vec<SessionAnalyzerDto>>, StatusCode> {
    use crate::analysis::analyzer::analyzer_display_name;

    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let overrides = fetch_session_analyzer_overrides(handle, session_uuid).await?;
    let snapshot = state
        .analyzer_registry()
        .load()
        .snapshot_with_overrides(&overrides);
    let hit_counts = fetch_session_diagnostic_hit_count_map(handle, session_uuid).await?;

    let mut rows: Vec<SessionAnalyzerDto> = snapshot
        .into_iter()
        .map(|entry| SessionAnalyzerDto {
            analyzer_name: entry.name.to_string(),
            display_name: analyzer_display_name(&entry.category).to_string(),
            enabled: entry.enabled,
            hit_count: hit_counts.get(entry.name).copied().unwrap_or(0),
            is_intercept: false,
        })
        .collect();

    rows.push(fetch_weasel_intercept_row(&state, handle, session_uuid, &overrides).await?);

    Ok(Json(rows))
}

#[derive(Serialize)]
pub(crate) struct SessionFilterRuleDto {
    pub(crate) rule_id: i64,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) tag: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) fired_turns: i64,
}

impl From<crate::store::persistence::filter_rules::SessionFilterRuleRow> for SessionFilterRuleDto {
    fn from(row: crate::store::persistence::filter_rules::SessionFilterRuleRow) -> Self {
        SessionFilterRuleDto {
            rule_id: row.rule_id,
            kind: row.kind,
            label: row.label,
            tag: row.tag,
            enabled: row.enabled,
            fired_turns: row.fired_turns,
        }
    }
}

pub(crate) async fn get_session_filter_rules(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
) -> Result<Json<Vec<SessionFilterRuleDto>>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    match handle.fetch_session_filter_rules(session_uuid).await {
        Ok(rows) => Ok(Json(
            rows.into_iter().map(SessionFilterRuleDto::from).collect(),
        )),
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "get_session_filter_rules: DB query failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PatchSessionAnalyzerBody {
    pub(crate) enabled: bool,
}

pub(crate) async fn patch_session_analyzer(
    State(state): State<AppState>,
    Path((session_uuid, analyzer_name)): Path<(uuid::Uuid, String)>,
    Json(body): Json<PatchSessionAnalyzerBody>,
) -> Response {
    let Some(handle) = state.persist() else {
        return persist_unavailable_response();
    };

    let known_names: std::collections::HashSet<&'static str> = state
        .analyzer_registry()
        .load()
        .snapshot()
        .iter()
        .map(|entry| entry.name)
        .chain(std::iter::once("weasel_intercept"))
        .collect();
    if !known_names.contains(analyzer_name.as_str()) {
        return (StatusCode::NOT_FOUND, "unknown analyzer name").into_response();
    }

    match handle
        .set_session_analyzer_override(session_uuid, &analyzer_name, body.enabled)
        .await
    {
        Ok(()) => Json(serde_json::json!({
            "analyzer_name": analyzer_name,
            "enabled": body.enabled,
        }))
        .into_response(),
        Err(crate::store::persistence::PersistError::DataInvariant(_)) => {
            (StatusCode::NOT_FOUND, "unknown session").into_response()
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                analyzer_name = %analyzer_name,
                "patch_session_analyzer: DB write failed"
            );
            persist_error_response(e)
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PostSessionMessageBody {
    #[serde(default, alias = "from", alias = "from_session_uuid")]
    pub(crate) from_session_id: Option<String>,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) thread_id: Option<String>,
    #[serde(default)]
    pub(crate) in_reply_to: Option<String>,
    #[serde(default)]
    pub(crate) subject: Option<String>,
    #[serde(default)]
    pub(crate) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) context: Vec<crate::store::persistence::mailbox::ContextItem>,
}
