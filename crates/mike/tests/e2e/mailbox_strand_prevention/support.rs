use crate::anthropic_fixtures;
use crate::test_db;

use std::net::SocketAddr;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::ResponseTemplate;

pub(super) use test_db::TestDb;

pub(super) async fn boot_proxy(
    upstream_url: &str,
    database_url: &str,
    extra_toml: &str,
) -> anyhow::Result<(AppState, String, String, CancellationToken)> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!("persist_enabled = true\ndatabase_url = \"{database_url}\"\n{extra_toml}"),
    )?;

    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("mailbox strand-prevention e2e proxy config extract: {e}"))?;
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
            MockResponse::new("msg_mailbox_strand_prevention")
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
            "Working directory: /tmp/mailbox-strand-prevention\n"
        )]))
        .content(json!([text_block(text)]))
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

pub(super) async fn post_message(
    client: &reqwest::Client,
    inspect_url: &str,
    to_session: uuid::Uuid,
    message: &str,
) -> anyhow::Result<(reqwest::StatusCode, Value)> {
    let resp = client
        .post(format!("{inspect_url}/api/sessions/{to_session}/message"))
        .json(&json!({"message": message}))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    Ok((status, body))
}

pub(super) async fn queue_mailbox_message(
    client: &reqwest::Client,
    inspect_url: &str,
    to_session: uuid::Uuid,
    message: &str,
) -> anyhow::Result<uuid::Uuid> {
    let (status, body) = post_message(client, inspect_url, to_session, message).await?;
    anyhow::ensure!(status == 201, "post_session_message returned {status}");
    let uuid_str = body["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("response missing message_uuid: {body:?}"))?;
    Ok(uuid::Uuid::parse_str(uuid_str)?)
}

pub(super) async fn ack_mailbox_message(
    client: &reqwest::Client,
    inspect_url: &str,
    to_session: uuid::Uuid,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(reqwest::StatusCode, Value)> {
    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{to_session}/messages/{message_uuid}/ack"
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    Ok((status, body))
}

pub(super) async fn mailbox_row_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(bool, bool, bool)> {
    let row = sqlx::query(
        "SELECT claimed_at, sent_request_id, consumed_at FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message_uuid)
    .fetch_one(pool)
    .await?;
    let claimed_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("claimed_at")?;
    let sent_request_id: Option<i64> = row.try_get("sent_request_id")?;
    let consumed_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("consumed_at")?;
    Ok((
        claimed_at.is_some(),
        sent_request_id.is_some(),
        consumed_at.is_some(),
    ))
}

pub(super) async fn seed_session(pool: &PgPool, session_uuid: uuid::Uuid) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(format!("/proj/mailbox-strand-prevention-{session_uuid}"))
    .fetch_one(pool)
    .await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(session_id)
}

pub(super) async fn seed_registration(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    lease_offset_secs: i64,
    ended: bool,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at, released_at)
         VALUES ($1, $2, '/tmp/mailbox-strand-prevention', 'rest', now() + make_interval(secs => $3),
             CASE WHEN $4 THEN now() ELSE NULL END)",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(lease_offset_secs)
    .bind(ended)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn count_failed_sends(
    pool: &PgPool,
    attempted_to: &str,
    reason: &str,
) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mailbox.failed_sends WHERE attempted_to = $1 AND reason = $2",
    )
    .bind(attempted_to)
    .bind(reason)
    .fetch_one(pool)
    .await?;
    Ok(count)
}
