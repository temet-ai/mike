use super::*;
use crate::anthropic_cli::{AnthropicCliError, LoginStatusDto, run_logout};

pub(crate) async fn get_anthropic_cli_status(
    State(state): State<AppState>,
) -> Json<crate::anthropic_cli::CliStatusDto> {
    let status = state
        .anthropic_cli
        .status_cache
        .get_or_refresh(&state.anthropic_cli.cli)
        .await;
    Json(status)
}

#[derive(Debug, Serialize)]
pub(crate) struct StartCliLoginResponse {
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) login: Option<LoginStatusDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fallback_command: Option<String>,
}

fn fallback_login_response(message: String) -> (StatusCode, Json<StartCliLoginResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(StartCliLoginResponse {
            ok: false,
            login: None,
            error: Some(message),
            fallback_command: Some("claude /login".to_owned()),
        }),
    )
}

pub(crate) async fn start_anthropic_cli_login(
    State(state): State<AppState>,
) -> (StatusCode, Json<StartCliLoginResponse>) {
    match state
        .anthropic_cli
        .logins
        .start(&state.anthropic_cli.cli)
        .await
    {
        Ok(login) => (
            StatusCode::OK,
            Json(StartCliLoginResponse {
                ok: true,
                login: Some(login),
                error: None,
                fallback_command: None,
            }),
        ),
        Err(AnthropicCliError::AlreadyRunning(_)) => {
            let login = state.anthropic_cli.logins.current_status().await;
            (
                StatusCode::CONFLICT,
                Json(StartCliLoginResponse {
                    ok: false,
                    login,
                    error: Some("a sign-in is already in progress".to_owned()),
                    fallback_command: None,
                }),
            )
        }
        Err(AnthropicCliError::NotFound) => fallback_login_response(
            "claude CLI not found on PATH — run the sign-in command yourself, or set \
             MIKE_CLAUDE_CLI_BIN and retry."
                .to_owned(),
        ),
        Err(e) => {
            fallback_login_response(format!("could not start claude CLI sign-in from mike: {e}"))
        }
    }
}

pub(crate) async fn get_anthropic_cli_login(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LoginStatusDto>, StatusCode> {
    state
        .anthropic_cli
        .logins
        .status(&id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

#[derive(Debug, Deserialize)]
pub(crate) struct SubmitCliLoginCodeBody {
    pub(crate) code: String,
}

pub(crate) async fn submit_anthropic_cli_login_code(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SubmitCliLoginCodeBody>,
) -> StatusCode {
    let code = body.code.trim();
    if code.is_empty() {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    match state.anthropic_cli.logins.submit_code(&id, code).await {
        Ok(()) => StatusCode::OK,
        Err(AnthropicCliError::NotRunning) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

pub(crate) async fn cancel_anthropic_cli_login(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    match state.anthropic_cli.logins.cancel(&id).await {
        Ok(()) => StatusCode::OK,
        Err(AnthropicCliError::NotRunning) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct AnthropicCliLogoutResponse {
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

pub(crate) async fn logout_anthropic_cli(
    State(state): State<AppState>,
) -> Json<AnthropicCliLogoutResponse> {
    let result = run_logout(&state.anthropic_cli.cli).await;
    state.anthropic_cli.status_cache.invalidate().await;
    match result {
        Ok(()) => Json(AnthropicCliLogoutResponse {
            ok: true,
            error: None,
        }),
        Err(e) => Json(AnthropicCliLogoutResponse {
            ok: false,
            error: Some(e.to_string()),
        }),
    }
}
