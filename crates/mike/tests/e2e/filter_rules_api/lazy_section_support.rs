use crate::anthropic_fixtures;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};

pub(super) const CHROME_HEADING: &str = "# Lazy-section e2e fixture heading";
pub(super) const CHROME_STUB: &str = "# Lazy-section e2e fixture heading\n\nGuidance on demand: output !!CHROME!! then call tabs_context_mcp.";
pub(super) const CHROME_TOKEN: &str = "!!CHROME!!";

pub(super) async fn create_lazy_chrome_rule(
    client: &reqwest::Client,
    inspect_url: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{inspect_url}/api/filter/rules"))
        .json(&json!({
            "kind": "lazy_section",
            "value": CHROME_HEADING,
            "action": "replace",
            "text": CHROME_STUB,
            "trigger_token": CHROME_TOKEN,
            "enabled": enabled,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 201,
        "lazy_section create must return 201, got {}: {}",
        resp.status(),
        resp.text().await?
    );
    Ok(())
}

pub(super) fn chrome_system_text() -> String {
    format!(
        "You are Claude Code, Anthropic's official CLI for Claude.\n\n\
         {CHROME_HEADING}\n\
         Use tabs_context_mcp before any other browser tool.\n\
         Never guess tab ids.\n\
         # Tone and style\n\
         Be concise."
    )
}

pub(super) fn lazy_request_body(messages: Value) -> Value {
    RequestBody::default()
        .max_tokens(100)
        .system(json!([
            text_block("billing header"),
            cached_text_block(chrome_system_text()),
        ]))
        .messages(messages)
        .build()
}
