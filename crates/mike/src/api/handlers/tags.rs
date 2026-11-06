use super::*;

pub(crate) async fn get_tags(
    State(state): State<AppState>,
) -> Result<Json<TagListResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; tags are not available".to_string(),
        ));
    };
    match handle.fetch_tags().await {
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
