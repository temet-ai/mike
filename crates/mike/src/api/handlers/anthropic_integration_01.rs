use secrecy::ExposeSecret;

use super::*;

pub(crate) async fn probe_anthropic_key(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<(), String> {
    let url = format!("{base_url}/v1/models");
    match client
        .get(&url)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => Err(format!(
            "Anthropic rejected the key (HTTP {})",
            resp.status().as_u16()
        )),
        Err(e) => Err(format!("request to Anthropic failed: {e}")),
    }
}

async fn fetch_anthropic_mode(state: &AppState) -> Option<String> {
    let svc = state.settings_service()?;
    let rows = svc.fetch_all().await.ok()?;
    rows.into_iter()
        .find(|(k, _)| k == "assistant.anthropic_auth_mode")
        .and_then(|(_, v)| v.as_str().map(str::to_owned))
}

fn default_anthropic_mode() -> String {
    crate::settings::SETTINGS_META
        .iter()
        .find(|m| m.key == "assistant.anthropic_auth_mode")
        .and_then(|m| m.default.as_str().map(str::to_owned))
        .unwrap_or_else(|| "claude_code_login".to_owned())
}

pub(crate) async fn resolve_anthropic_mode(state: &AppState) -> String {
    fetch_anthropic_mode(state)
        .await
        .unwrap_or_else(default_anthropic_mode)
}

#[derive(Debug, Serialize)]
pub(crate) struct AnthropicIntegrationStatus {
    pub(crate) mode: String,
    pub(crate) configured: bool,
    pub(crate) masked_key: Option<String>,
}

pub(crate) async fn get_anthropic_integration(
    State(state): State<AppState>,
) -> Json<AnthropicIntegrationStatus> {
    let mode = resolve_anthropic_mode(&state).await;
    let key = crate::llm::key::resolve(&state.anthropic_api_key);
    let masked_key = key.map(|k| mask_openrouter_key(k.expose_secret()));
    Json(AnthropicIntegrationStatus {
        mode,
        configured: masked_key.is_some(),
        masked_key,
    })
}
