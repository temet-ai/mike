use std::net::SocketAddr;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
use chrono::{DateTime, Utc};
use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
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
    let mut config: ProxyConfig = base.extract().map_err(|e| {
        anyhow::anyhow!("mailbox busy-recipient-delivery e2e proxy config extract: {e}")
    })?;
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
            MockResponse::new("msg_mailbox_busy_recipient")
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
            "Working directory: /tmp/mailbox-busy-recipient\n"
        )]))
        .content(json!([text_block(text)]))
        .build()
}

pub(super) async fn wait_for_session_id(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<i64> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let id: Option<i64> = sqlx::query_scalar!(
            "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
            session_uuid
        )
        .fetch_optional(pool)
        .await?;
        if let Some(id) = id {
            return Ok(id);
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

pub(super) async fn backdate_queued_at(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<()> {
    sqlx::query!(
        "UPDATE mailbox.messages SET queued_at = now() - INTERVAL '2 hours' WHERE message_uuid = $1",
        message_uuid,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn seed_captured_request(
    pool: &PgPool,
    session_id: i64,
    turn_origin_code: &str,
    created_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"INSERT INTO requests.requests
               (session_id, project_id, method_id, path, is_streaming, message_count,
                is_tool_loop, status_id, params, last_user_message, human_prompt,
                turn_type_id, turn_origin_id, created_at)
           SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 1, false, rs.id,
                  '{}'::jsonb, '', '', rtt.id, rto.id, $3
             FROM sessions.sessions s, requests.methods rm, requests.statuses rs,
                  requests.turn_types rtt, requests.turn_origins rto
            WHERE s.id = $1 AND rm.code = 'POST' AND rs.code = 'complete'
              AND rtt.code = 'user_prompt' AND rto.code = $2"#,
        session_id,
        turn_origin_code,
        created_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn mailbox_row_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(bool, bool)> {
    let row = sqlx::query!(
        "SELECT claimed_at, sent_request_id FROM mailbox.messages WHERE message_uuid = $1",
        message_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.claimed_at.is_some(), row.sent_request_id.is_some()))
}

pub(super) async fn mailbox_reap_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(i32, bool)> {
    let row = sqlx::query!(
        "SELECT delivery_attempts, dead_lettered_at FROM mailbox.messages WHERE message_uuid = $1",
        message_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.delivery_attempts, row.dead_lettered_at.is_some()))
}

pub(super) async fn seed_silent_session(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<()> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        format!("/proj/mailbox-busy-recipient-silent-{session_uuid}"),
    )
    .fetch_one(pool)
    .await?;
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())",
        session_uuid,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn failed_sends_count(
    pool: &PgPool,
    attempted_to: &str,
    reason: &str,
) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM mailbox.failed_sends WHERE attempted_to = $1 AND reason = $2"#,
        attempted_to,
        reason,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
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

pub(super) async fn wait_for_dead_letter(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let (_, dead_lettered) = mailbox_reap_state(pool, message_uuid).await?;
        if dead_lettered {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for message {message_uuid} to be dead-lettered by the reaper"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
