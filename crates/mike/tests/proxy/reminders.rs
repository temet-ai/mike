use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody, assistant_message, text_block, user_message};
use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct StartedRemindersProxy {
    proxy_url: String,
    _inspect_url: String,
    _tmp: tempfile::TempDir,
    shutdown: CancellationToken,
}

impl Drop for StartedRemindersProxy {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn start_proxy_with_reminders(upstream_url: &str) -> anyhow::Result<StartedRemindersProxy> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        "persist_enabled = false\n\n[reminders]\nenabled = true\n",
    )?;

    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("reminders proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = upstream_url.to_string();

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });
    tokio::task::yield_now().await;

    Ok(StartedRemindersProxy {
        proxy_url: format!("http://{proxy_addr}"),
        _inspect_url: format!("http://{inspect_addr}"),
        _tmp: tmp,
        shutdown,
    })
}

fn weasel_response_json() -> Value {
    MockResponse::new("msg_first_with_finding")
        .text("Here is a comprehensive solution that handles all cases.")
        .usage(10, 12)
        .build()
}

fn plain_response_json() -> Value {
    MockResponse::new("msg_second_clean")
        .text("Done.")
        .usage(6, 1)
        .build()
}

#[tokio::test]
async fn reminder_appended_to_second_request_after_finding_on_first() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(weasel_response_json())
                .insert_header("content-type", "application/json"),
        )
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(plain_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = start_proxy_with_reminders(&mock_server.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "11111111-1111-1111-1111-111111111111";

    send_first_turn_and_wait_for_finding(&client, &proxy, session_id).await?;
    send_second_turn_with_prior_assistant_reply(&client, &proxy, session_id).await?;

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 2, "exactly two upstream requests expected");

    assert_reminder_appended_to_second_upstream_request(&received[1].body)?;
    assert_first_upstream_request_unmutated(&received[0].body)?;

    Ok(())
}

async fn send_first_turn_and_wait_for_finding(
    client: &reqwest::Client,
    proxy: &StartedRemindersProxy,
    session_id: &str,
) -> anyhow::Result<()> {
    let first_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([text_block("You are Claude.")]))
        .messages(json!([user_message(json!([text_block("first turn")]))]))
        .build();
    let first_resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(serde_json::to_vec(&first_body)?)
        .send()
        .await?;
    assert_eq!(first_resp.status(), 200);
    let _ = first_resp.bytes().await?;

    let inspect_url = proxy._inspect_url.clone();
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(2) {
            anyhow::bail!("analyzer never attached findings within 2s budget");
        }
        let list = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?;
        if list.status() == 200 {
            let arr: Value = list.json().await?;
            if let Some(rid) = arr
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["id"].as_u64())
            {
                let detail = client
                    .get(format!("{inspect_url}/api/requests/{rid}"))
                    .send()
                    .await?;
                if detail.status() == 200 {
                    let d: Value = detail.json().await?;
                    if d["diagnostics"].as_array().is_some_and(|a| !a.is_empty()) {
                        return Ok(());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn send_second_turn_with_prior_assistant_reply(
    client: &reqwest::Client,
    proxy: &StartedRemindersProxy,
    session_id: &str,
) -> anyhow::Result<()> {
    let second_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([text_block("You are Claude.")]))
        .messages(json!([
            user_message(json!([text_block("first turn")])),
            assistant_message(json!([text_block(
                "Here is a comprehensive solution that handles all cases."
            )])),
            user_message(json!([text_block("follow-up")])),
        ]))
        .build();
    let second_resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(serde_json::to_vec(&second_body)?)
        .send()
        .await?;
    assert_eq!(second_resp.status(), 200);
    let _ = second_resp.bytes().await?;
    Ok(())
}

fn assert_reminder_appended_to_second_upstream_request(body: &[u8]) -> anyhow::Result<()> {
    let upstream_second_body: Value = serde_json::from_slice(body)?;
    let messages = upstream_second_body["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("upstream body is missing `messages` array"))?;
    assert_eq!(
        messages.len(),
        3,
        "reminder must NOT introduce a new turn — only append a content block"
    );
    let last_content = messages[2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("last turn `content` must be an array"))?;
    assert_eq!(
        last_content.len(),
        2,
        "exactly one block must be appended to the last user turn"
    );
    assert_eq!(last_content[0]["text"], "follow-up");
    assert_eq!(last_content[1]["type"], "text");
    let reminder = last_content[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("appended block missing string `text`"))?;
    assert!(
        reminder.contains("<system-reminder source=\"proxy-reminders\">"),
        "appended block must carry the proxy's self-identifying wrapper, got: {reminder}"
    );
    assert!(
        reminder.contains("</system-reminder>"),
        "appended block must be properly closed, got: {reminder}"
    );
    assert!(
        reminder.contains("comprehensive"),
        "appended block must mention the matched_text from the prior finding, got: {reminder}"
    );
    Ok(())
}

fn assert_first_upstream_request_unmutated(body: &[u8]) -> anyhow::Result<()> {
    let upstream_first_body: Value = serde_json::from_slice(body)?;
    let first_msgs = upstream_first_body["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("first upstream body missing messages"))?;
    let first_content = first_msgs[0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("first turn content array missing"))?;
    assert_eq!(
        first_content.len(),
        1,
        "first request must be byte-identical w.r.t. message count — no prior session findings"
    );
    Ok(())
}

#[tokio::test]
async fn reminder_when_armed_appends_block_on_followup() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(weasel_response_json())
                .insert_header("content-type", "application/json"),
        )
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(plain_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = start_proxy_with_reminders(&mock_server.uri()).await?;
    let proxy_url = proxy.proxy_url.clone();
    let client = reqwest::Client::new();
    let session_id = "22222222-2222-2222-2222-222222222222";

    let first_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([text_block("You are Claude.")]))
        .messages(json!([user_message(json!([text_block("first")]))]))
        .build();
    let r1 = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(serde_json::to_vec(&first_body)?)
        .send()
        .await?;
    assert_eq!(r1.status(), 200);
    let _ = r1.bytes().await?;

    let second_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([text_block("You are Claude.")]))
        .messages(json!([
            user_message(json!([text_block("first")])),
            assistant_message(json!([text_block("comprehensive")])),
            user_message(json!([text_block("follow-up")])),
        ]))
        .build();
    let second_bytes = serde_json::to_vec(&second_body)?;
    let r2 = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(second_bytes.clone())
        .send()
        .await?;
    assert_eq!(r2.status(), 200);
    let _ = r2.bytes().await?;

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 2);

    let upstream_first_body: Value = serde_json::from_slice(&received[0].body)?;
    let first_content = upstream_first_body["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("first turn content array missing"))?;
    assert_eq!(
        first_content.len(),
        1,
        "first request must be byte-identical — no prior session findings"
    );

    assert_ne!(
        received[1].body, second_bytes,
        "reminders armed: second-request body must be rewritten"
    );
    let upstream_second_body: Value = serde_json::from_slice(&received[1].body)?;
    let second_msgs = upstream_second_body["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("second upstream body missing messages"))?;
    let last_content = second_msgs
        .last()
        .and_then(|m| m["content"].as_array())
        .ok_or_else(|| anyhow::anyhow!("last turn content array missing"))?;
    let appended = last_content
        .last()
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("appended block text missing"))?;
    assert!(
        appended.contains("<system-reminder source=\"proxy-reminders\">"),
        "second request must carry the reminder block, got: {appended}"
    );

    Ok(())
}
