use secrecy::ExposeSecret;

use super::*;

#[derive(Deserialize)]
pub(crate) struct PutAnthropicKeyBody {
    pub(crate) key: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PutAnthropicKeyResponse {
    pub(crate) valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    pub(crate) persisted: bool,
}

fn apply_anthropic_key(state: &AppState, key: &str) {
    state
        .anthropic_api_key
        .store(std::sync::Arc::new(Some(secrecy::SecretString::new(
            key.to_owned().into(),
        ))));
    tracing::info!(
        "anthropic api key: applied to this process only; secrets are never written to the \
         settings store, so set MIKE_ASSISTANT_ANTHROPIC_API_KEY in the environment for it to \
         survive a restart"
    );
}

pub(crate) async fn put_anthropic_key(
    State(state): State<AppState>,
    Json(body): Json<PutAnthropicKeyBody>,
) -> (StatusCode, Json<PutAnthropicKeyResponse>) {
    let key = body.key.trim().to_owned();
    if key.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PutAnthropicKeyResponse {
                valid: false,
                label: None,
                error: Some("key must not be empty".to_owned()),
                persisted: false,
            }),
        );
    }

    let base_url = state.config().upstream_url.clone();
    if let Err(reason) = probe_anthropic_key(&state.client, &base_url, &key).await {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PutAnthropicKeyResponse {
                valid: false,
                label: None,
                error: Some(reason),
                persisted: false,
            }),
        );
    }

    apply_anthropic_key(&state, &key);

    (
        StatusCode::OK,
        Json(PutAnthropicKeyResponse {
            valid: true,
            label: Some(mask_openrouter_key(&key)),
            error: None,
            persisted: false,
        }),
    )
}

#[derive(Debug, Serialize)]
pub(crate) struct ClearAnthropicKeyResponse {
    pub(crate) cleared: bool,
}

pub(crate) async fn clear_anthropic_key(
    State(state): State<AppState>,
) -> Json<ClearAnthropicKeyResponse> {
    state.anthropic_api_key.store(std::sync::Arc::new(None));
    Json(ClearAnthropicKeyResponse { cleared: true })
}

#[derive(Deserialize)]
pub(crate) struct TestAnthropicConnectionBody {
    pub(crate) key: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TestAnthropicConnectionResponse {
    pub(crate) ok: bool,
    pub(crate) message: String,
}

async fn test_explicit_key(state: &AppState, key: &str) -> TestAnthropicConnectionResponse {
    let base_url = state.config().upstream_url.clone();
    match probe_anthropic_key(&state.client, &base_url, key).await {
        Ok(()) => TestAnthropicConnectionResponse {
            ok: true,
            message: "Anthropic accepted the key.".to_owned(),
        },
        Err(reason) => TestAnthropicConnectionResponse {
            ok: false,
            message: reason,
        },
    }
}

async fn test_stored_key(state: &AppState) -> TestAnthropicConnectionResponse {
    let Some(key) = crate::llm::key::resolve(&state.anthropic_api_key) else {
        return TestAnthropicConnectionResponse {
            ok: false,
            message: "No Anthropic API key is saved — add one first.".to_owned(),
        };
    };
    test_explicit_key(state, key.expose_secret()).await
}

async fn test_current_configuration(state: &AppState) -> TestAnthropicConnectionResponse {
    let mode = resolve_anthropic_mode(state).await;
    if mode == "api_key" {
        return test_stored_key(state).await;
    }
    TestAnthropicConnectionResponse {
        ok: true,
        message: "Using the Claude Code login on this machine — validated by the assistant's \
                   own turns, not by this check."
            .to_owned(),
    }
}

pub(crate) async fn test_anthropic_connection(
    State(state): State<AppState>,
    Json(body): Json<TestAnthropicConnectionBody>,
) -> Json<TestAnthropicConnectionResponse> {
    let explicit = body.key.as_deref().map(str::trim).filter(|k| !k.is_empty());
    match explicit {
        Some(key) => Json(test_explicit_key(&state, key).await),
        None => Json(test_current_configuration(&state).await),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct AnthropicCredentialsResponse {
    pub(crate) mode: String,
    pub(crate) api_key: Option<String>,
}

fn carries_service_token(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get(crate::service_token::HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|candidate| state.service_token.matches(candidate))
}

pub(crate) async fn get_anthropic_credentials_for_service(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !carries_service_token(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "this route serves the saved API key and needs the service token \
                          mike publishes in its state directory"
            })),
        )
            .into_response();
    }
    let mode = resolve_anthropic_mode(&state).await;
    let api_key = if mode == "api_key" {
        crate::llm::key::resolve(&state.anthropic_api_key).map(|k| k.expose_secret().to_owned())
    } else {
        None
    };
    Json(AnthropicCredentialsResponse { mode, api_key }).into_response()
}
