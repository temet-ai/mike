use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::tool_uses::{ToolUseCount, ToolUseGroup};

#[derive(Debug, Serialize)]
pub(crate) struct ToolUseCountsResponse {
    pub(crate) group_by: &'static str,
    pub(crate) total: i64,
    pub(crate) groups: Vec<ToolUseCount>,
}

pub(crate) async fn count_tool_uses(
    State(state): State<AppState>,
    Query(params): Query<ToolUsesFeedParams>,
) -> Result<Json<ToolUseCountsResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let group = params
        .group_by
        .as_deref()
        .and_then(ToolUseGroup::parse)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "error": format!("group_by must be {}", ToolUseGroup::NAMES),
                })),
            )
                .into_response()
        })?;
    let filter = feed_filter(params)?;
    let groups = handle
        .launch()
        .count_tool_uses_by(&filter, group)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "count_tool_uses: query failed");
            persist_error_response(e)
        })?;
    Ok(Json(ToolUseCountsResponse {
        group_by: group.as_str(),
        total: groups.iter().map(|g| g.count).sum(),
        groups,
    }))
}
