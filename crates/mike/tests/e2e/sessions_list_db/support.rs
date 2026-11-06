use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(super) async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .content(user_text)
        .max_tokens(4)
        .build();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(response.status() == 200);
    let _ = response.bytes().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

pub(super) fn mock_response() -> Value {
    MockResponse::new("msg_a3_sessions_list")
        .text("ok")
        .usage(1, 1)
        .build()
}

pub(super) async fn mount_upstream() -> MockServer {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(mock_response())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;
    mock_server
}
