use super::*;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct LessonsListParams {
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) order: Option<SortOrder>,
    #[serde(default)]
    pub(crate) topic: Option<String>,
}

pub(crate) async fn fetch_lessons_for_project(
    handle: &crate::store::persistence::PersistHandle,
    project: &crate::store::persistence::project_knowledge::ProjectIdentity,
    order: SortOrder,
    topic: Option<&str>,
) -> Result<Vec<LessonDto>, crate::store::persistence::PersistError> {
    let mut rows = handle.lessons().list_active_lessons(project.id).await?;
    if let Some(want) = topic {
        rows.retain(|r| r.topic.as_deref() == Some(want));
    }
    if order == SortOrder::Asc {
        rows.reverse();
    }
    Ok(rows
        .into_iter()
        .map(|r| LessonDto {
            id: r.id,
            project_id: r.project_id,
            project_path: r.project_path,
            kind: r.kind,
            text: r.text,
            source_turn_id: r.source_turn_id,
            created_at: r.created_at.to_rfc3339(),
            expires_at: r.expires_at.map(|t| t.to_rfc3339()),
            topic: r.topic,
        })
        .collect())
}

pub(crate) async fn list_lessons(
    State(state): State<AppState>,
    Query(params): Query<LessonsListParams>,
) -> Result<Json<Vec<LessonDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Some(project_path) = params.project else {
        return Err(lesson_error(
            StatusCode::BAD_REQUEST,
            "missing required `project` query parameter",
        ));
    };
    let project = handle
        .knowledge()
        .resolve_knowledge_project(&project_path)
        .await
        .map_err(crate::api::project_knowledge::knowledge_error)?;
    let lessons = fetch_lessons_for_project(
        handle,
        &project,
        params.order.unwrap_or_default(),
        params.topic.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::warn!(project = %project_path, error = %e, "list_lessons: query failed");
        persist_error_response(e)
    })?;
    Ok(Json(lessons))
}

pub(crate) async fn delete_lesson(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let parsed = id.parse::<i64>().map_err(|_| {
        lesson_error(
            StatusCode::BAD_REQUEST,
            "`id` must be an integer — see `memory.lessons.id`",
        )
    })?;
    let deleted = handle.lessons().delete_lesson(parsed).await.map_err(|e| {
        tracing::warn!(lesson_id = %id, error = %e, "delete_lesson: query failed");
        persist_error_response(e)
    })?;
    if !deleted {
        return Err(lesson_error(StatusCode::NOT_FOUND, "lesson not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
