use super::*;

pub(crate) async fn get_models(
    State(state): State<AppState>,
) -> Result<Json<ModelsResponse>, (StatusCode, Json<serde_json::Value>)> {
    match state.model_catalogue().get().await {
        Ok(models) => Ok(Json(ModelsResponse { models })),
        Err(e) => {
            tracing::warn!(error = %e, "get_models: OpenRouter fetch failed");
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
