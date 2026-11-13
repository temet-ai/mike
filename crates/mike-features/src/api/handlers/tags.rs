use crate::api::handlers::mailbox_dto::mailbox_error;
use crate::api::handlers::mailbox_thread_dto::TagDto;
use crate::api::handlers::mailbox_thread_dto::TagListResponse;
use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

pub async fn get_tags(State(state): State<BaseState>) -> Result<Json<TagListResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; tags are not available".to_string(),
        ));
    };
    match handle.launch().fetch_tags().await {
        Ok(rows) => Ok(Json(TagListResponse {
            tags: rows
                .into_iter()
                .map(|r| TagDto {
                    code: r.code,
                    hue: r.hue,
                    usage_count: r.usage_count,
                })
                .collect(),
        })),
        Err(e) => {
            tracing::warn!(error = %e, "get_tags: fetch failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch tags".to_string(),
            ))
        }
    }
}
