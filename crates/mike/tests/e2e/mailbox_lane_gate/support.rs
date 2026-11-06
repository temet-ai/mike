use std::net::SocketAddr;

use anthropic_fixtures::{
    MockResponse, RequestBody, assistant_message, text_block, tool_result_block, tool_use_block,
    user_message,
};
use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::ResponseTemplate;

use crate::anthropic_fixtures;

pub(super) async fn boot_proxy(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(AppState, String, String, CancellationToken)> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!("persist_enabled = true\ndatabase_url = \"{database_url}\"\n"),
    )?;

    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("mailbox lane-gate e2e proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = upstream_url.to_string();

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr: SocketAddr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr: SocketAddr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);

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

    Ok((
        state,
        format!("http://{proxy_addr}"),
        format!("http://{inspect_addr}"),
        shutdown,
    ))
}

pub(super) fn ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(
            MockResponse::new("msg_mailbox_lane_gate")
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

pub(super) fn primary_body(text: &str) -> Value {
    RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(
            "Working directory: /tmp/mailbox-lane-gate\n"
        )]))
        .content(json!([text_block(text)]))
        .build()
}

pub(super) fn subagent_body(text: &str) -> Value {
    RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(
            "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Rust Development Expert -- a senior engineer who writes production-grade code.\n"
        )]))
        .content(json!([text_block(text)]))
        .build()
}

pub(super) fn workflow_subagent_tool_loop_body(text: &str) -> Value {
    RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(
            "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Verification Specialist.\nYou are running inside a workflow script. The current working directory is /tmp/mailbox-lane-gate.\n"
        )]))
        .messages(json!([
            user_message(json!([text_block(text)])),
            assistant_message(json!([tool_use_block(
                "toolu_1",
                "ReportFindings",
                json!({})
            )])),
            user_message(json!([tool_result_block("toolu_1", "ok")])),
        ]))
        .build()
}

pub(super) async fn wait_for_session_row(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
                .bind(session_uuid)
                .fetch_optional(pool)
                .await?;
        if id.is_some() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for session row {session_uuid}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

pub(super) async fn queue_mailbox_message(
    client: &reqwest::Client,
    inspect_url: &str,
    to_session: uuid::Uuid,
    message: &str,
) -> anyhow::Result<uuid::Uuid> {
    let resp = client
        .post(format!("{inspect_url}/api/sessions/{to_session}/message"))
        .json(&json!({"message": message}))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 201,
        "post_session_message returned {}",
        resp.status()
    );
    let envelope: Value = resp.json().await?;
    let uuid_str = envelope["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("response missing message_uuid: {envelope:?}"))?;
    Ok(uuid::Uuid::parse_str(uuid_str)?)
}

pub(super) async fn mailbox_row_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(bool, bool)> {
    let row = sqlx::query(
        "SELECT claimed_at, sent_request_id FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message_uuid)
    .fetch_one(pool)
    .await?;
    let claimed_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("claimed_at")?;
    let sent_request_id: Option<i64> = row.try_get("sent_request_id")?;
    Ok((claimed_at.is_some(), sent_request_id.is_some()))
}

pub(super) async fn mailbox_redelivery_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(i32, bool, bool)> {
    let row = sqlx::query(
        "SELECT delivery_attempts, consumed_at, dead_lettered_at
           FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message_uuid)
    .fetch_one(pool)
    .await?;
    let delivery_attempts: i32 = row.try_get("delivery_attempts")?;
    let consumed_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("consumed_at")?;
    let dead_lettered_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("dead_lettered_at")?;
    Ok((
        delivery_attempts,
        consumed_at.is_some(),
        dead_lettered_at.is_some(),
    ))
}

pub(super) async fn ack_mailbox_message(
    client: &reqwest::Client,
    inspect_url: &str,
    to_session: uuid::Uuid,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<reqwest::StatusCode> {
    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{to_session}/messages/{message_uuid}/ack"
        ))
        .send()
        .await?;
    Ok(resp.status())
}

pub(super) fn last_block_text(forwarded: &Value) -> anyhow::Result<String> {
    let messages = forwarded["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("forwarded body missing messages array"))?;
    let last_msg = messages
        .last()
        .ok_or_else(|| anyhow::anyhow!("messages array is empty"))?;
    let content = last_msg["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("last message content is not an array"))?;
    let last_block = content
        .last()
        .ok_or_else(|| anyhow::anyhow!("last message content is empty"))?;
    Ok(last_block["text"].as_str().unwrap_or("").to_string())
}
