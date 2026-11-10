use super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RecallProjectContextDto {
    pub(crate) project_path: String,
    pub(crate) recent_corrected: Vec<OperatorTurnDto>,
    pub(crate) recent_shipped: Vec<OperatorTurnDto>,
    pub(crate) lessons: Vec<LessonDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) knowledge: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RecallProjectContextParams {
    pub(crate) project: Option<String>,
    pub(crate) query: Option<String>,
}

pub(crate) const RECALL_RECENT_CORRECTED_LIMIT: i64 = 5;
pub(crate) const RECALL_RECENT_SHIPPED_LIMIT: i64 = 3;

fn recent_turns_filter(
    project_path: &str,
    outcome: Option<&str>,
    limit: i64,
) -> crate::store::persistence::operator_turns::ListTurnsFilter {
    crate::store::persistence::operator_turns::ListTurnsFilter {
        project_path: Some(project_path.to_owned()),
        session_db_id: None,
        since: None,
        until: None,
        outcome: outcome.map(str::to_owned),
        correction: outcome.is_none(),
        subject_command: None,
        subject_agent: None,
        skill: None,
        limit,
        offset: 0,
        order: SortOrder::Desc,
    }
}

pub(crate) async fn recall_project_context(
    State(state): State<AppState>,
    Query(params): Query<RecallProjectContextParams>,
) -> Result<Json<RecallProjectContextDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Some(project_path) = params.project else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "missing required `project` query parameter",
            })),
        )
            .into_response());
    };

    let project = handle
        .resolve_knowledge_project(&project_path)
        .await
        .map_err(crate::api::project_knowledge::knowledge_error)?;
    let project_path = project.project_path.clone();

    let corrected_rows = handle
        .list_turns(&recent_turns_filter(&project_path, None, RECALL_RECENT_CORRECTED_LIMIT))
        .await
        .map_err(|e| {
            tracing::warn!(project = %project_path, error = %e, "recall_project_context: corrected turns failed");
            persist_error_response(e)
        })?;

    let shipped_rows = handle
        .list_turns(&recent_turns_filter(
            &project_path,
            Some("shipped"),
            RECALL_RECENT_SHIPPED_LIMIT,
        ))
        .await
        .map_err(|e| {
            tracing::warn!(project = %project_path, error = %e, "recall_project_context: shipped turns failed");
            persist_error_response(e)
        })?;

    let lessons = fetch_lessons_for_project(handle, &project, SortOrder::Desc, None)
        .await
        .map_err(|e| {
            tracing::warn!(project = %project_path, error = %e, "recall_project_context: lessons failed");
            persist_error_response(e)
        })?;

    let knowledge = if let Some(query) = params.query {
        Some(
            crate::api::project_knowledge::retrieve_without_rerank(
                &state,
                &crate::store::persistence::project_knowledge::KnowledgeQuery {
                    project: project_path.clone(),
                    query,
                    limit: Some(5),
                    ..Default::default()
                },
            )
            .await?,
        )
    } else {
        None
    };
    Ok(Json(RecallProjectContextDto {
        knowledge,
        project_path,
        recent_corrected: corrected_rows
            .into_iter()
            .map(OperatorTurnDto::from)
            .collect(),
        recent_shipped: shipped_rows
            .into_iter()
            .map(OperatorTurnDto::from)
            .collect(),
        lessons,
    }))
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RecallFileHistoryParams {
    pub(crate) project: Option<String>,
    pub(crate) file: Option<String>,
    pub(crate) limit: Option<usize>,
}

pub(crate) const RECALL_FILE_HISTORY_DEFAULT_LIMIT: i64 = 5;
pub(crate) const RECALL_FILE_HISTORY_MAX_LIMIT: i64 = 100;

pub(crate) async fn recall_file_history(
    State(state): State<AppState>,
    Query(params): Query<RecallFileHistoryParams>,
) -> Result<Json<Vec<OperatorTurnDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Some(project_path) = params.project.as_deref() else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "missing required `project` query parameter",
            })),
        )
            .into_response());
    };
    let Some(file) = params.file.as_deref() else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "missing required `file` query parameter",
            })),
        )
            .into_response());
    };
    let limit = params
        .limit
        .map(|l| l as i64)
        .unwrap_or(RECALL_FILE_HISTORY_DEFAULT_LIMIT)
        .clamp(1, RECALL_FILE_HISTORY_MAX_LIMIT);

    let project = handle
        .resolve_knowledge_project(project_path)
        .await
        .map_err(crate::api::project_knowledge::knowledge_error)?;
    let rows = handle
        .recall_turns_by_file(&project, file, limit)
        .await
        .map_err(|e| {
            tracing::warn!(
                project = %project_path,
                file = %file,
                error = %e,
                "recall_file_history: query failed"
            );
            persist_error_response(e)
        })?;

    let dtos: Vec<OperatorTurnDto> = rows.into_iter().map(OperatorTurnDto::from).collect();
    Ok(Json(dtos))
}
