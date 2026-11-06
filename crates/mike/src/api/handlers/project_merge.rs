use super::*;
use crate::store::persistence::project_merge::ProjectMergeError;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectMergeRequest {
    pub(crate) into_project_uuid: Uuid,
}

pub(crate) async fn post_project_merge(
    State(state): State<AppState>,
    Path(project_path): Path<String>,
    Json(body): Json<ProjectMergeRequest>,
) -> Response {
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    match persist
        .merge_project_into(&project_path, body.into_project_uuid)
        .await
    {
        Ok(merged) => Json(merged).into_response(),
        Err(ProjectMergeError::UnknownSource) => {
            (StatusCode::NOT_FOUND, "unknown project").into_response()
        }
        Err(ProjectMergeError::UnknownTarget) => {
            (StatusCode::NOT_FOUND, "unknown target project").into_response()
        }
        Err(ProjectMergeError::SameProject) => (
            StatusCode::BAD_REQUEST,
            "a project cannot be made part of itself",
        )
            .into_response(),
        Err(ProjectMergeError::Db(e)) => persist_error_response(e),
    }
}
