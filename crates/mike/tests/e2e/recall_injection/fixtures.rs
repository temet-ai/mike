use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody, assistant_message, text_block, user_message};
use serde_json::{Value, json};
use wiremock::ResponseTemplate;

pub(super) fn ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(
            MockResponse::new("msg_recall")
                .text("ok")
                .usage(1, 1)
                .build(),
        )
        .insert_header("content-type", "application/json")
}

pub(super) async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    body: &Value,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(serde_json::to_vec(body)?)
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "proxy returned {}", resp.status());
    let _ = resp.bytes().await?;
    Ok(())
}

pub(super) fn body_with_recall_tag(working_dir: &str, topic: &str, user_text: &str) -> Value {
    RequestBody::default()
        .system(json!([text_block(format!(
            "You are Claude.\n\nWorking directory: {working_dir}\n"
        ))]))
        .messages(json!([
            user_message(json!([text_block("initial question")])),
            assistant_message(json!([text_block(format!(
                "I need to recall. <mem:recall topic=\"{topic}\"/>"
            ))])),
            user_message(json!([text_block(user_text)])),
        ]))
        .max_tokens(64)
        .build()
}
