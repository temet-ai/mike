use super::*;

async fn apply_openrouter_key(state: &AppState, key: &str) {
    state
        .steerer_openrouter_key
        .store(std::sync::Arc::new(Some(secrecy::SecretString::new(
            key.to_owned().into(),
        ))));
    state.model_catalogue().bust_cache().await;
    tracing::info!(
        "steerer openrouter key: applied to this process only; secrets are never written to \
         the settings store, so set MIKE_STEERER_OPENROUTER_API_KEY in the environment for it \
         to survive a restart"
    );
}

pub(crate) async fn put_steerer_openrouter_key(
    State(state): State<AppState>,
    Json(body): Json<PutOpenRouterKeyBody>,
) -> (StatusCode, Json<PutOpenRouterKeyResponse>) {
    let key = body.key.trim().to_owned();
    if key.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PutOpenRouterKeyResponse {
                valid: false,
                label: None,
                error: Some("key must not be empty".to_owned()),
                persisted: false,
            }),
        );
    }

    let base_url = state.config().steerer.openrouter_base_url.clone();
    if let Err(reason) = probe_openrouter_key(&state.client, &base_url, &key).await {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PutOpenRouterKeyResponse {
                valid: false,
                label: None,
                error: Some(reason),
                persisted: false,
            }),
        );
    }

    apply_openrouter_key(&state, &key).await;

    (
        StatusCode::OK,
        Json(PutOpenRouterKeyResponse {
            valid: true,
            label: Some(mask_openrouter_key(&key)),
            error: None,
            persisted: false,
        }),
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelUsageQuery {
    pub(crate) days: Option<i32>,
}
