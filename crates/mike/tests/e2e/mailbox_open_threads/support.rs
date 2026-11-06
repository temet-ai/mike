use std::net::SocketAddr;

use crate::anthropic_fixtures::{MockResponse, RequestBody, text_block};
use chrono::{DateTime, Utc};
use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::ResponseTemplate;

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
        .map_err(|e| anyhow::anyhow!("mailbox_open_threads e2e proxy config extract: {e}"))?;
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
            MockResponse::new("msg_open_threads")
                .text("ok")
                .usage(1, 1)
                .build(),
        )
        .insert_header("content-type", "application/json")
}

pub(super) async fn send_primary_turn(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(
            "Working directory: /tmp/open-threads\n"
        )]))
        .content(json!([text_block("hello")]))
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(serde_json::to_vec(&body)?)
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "proxy returned {}", resp.status());
    let _ = resp.bytes().await?;
    Ok(())
}

pub(super) async fn mailbox_row_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(bool, bool, bool)> {
    let row = sqlx::query!(
        "SELECT claimed_at, sent_request_id, dead_lettered_at
           FROM mailbox.messages WHERE message_uuid = $1",
        message_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((
        row.claimed_at.is_some(),
        row.sent_request_id.is_some(),
        row.dead_lettered_at.is_some(),
    ))
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

pub(super) struct ToolUseSeed<'a> {
    pub wire_id: &'a str,
    pub request_id: i64,
    pub ord: i32,
    pub session_db_id: i64,
    pub tool_name: &'a str,
    pub input: Value,
    pub created_at: DateTime<Utc>,
}

pub(super) async fn seed_tool_use(pool: &PgPool, seed: ToolUseSeed<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(seed.wire_id)
    .bind(seed.request_id)
    .bind(seed.ord)
    .bind(seed.tool_name)
    .bind(seed.input)
    .bind(seed.session_db_id)
    .bind(seed.created_at)
    .execute(pool)
    .await?;
    Ok(())
}
