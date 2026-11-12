use crate::test_db;

use std::net::SocketAddr;

use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub(super) use test_db::TestDb;

pub(super) async fn boot_proxy(
    database_url: &str,
    mailbox_stale_minutes: i64,
) -> anyhow::Result<(String, CancellationToken)> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!(
            "persist_enabled = true\ndatabase_url = \"{database_url}\"\nmailbox_stale_minutes = {mailbox_stale_minutes}\n"
        ),
    )?;

    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("mailbox staleness e2e proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = "http://127.0.0.1:1".to_string();

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr: SocketAddr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });
    tokio::task::yield_now().await;

    Ok((format!("http://{inspect_addr}"), shutdown))
}

pub(super) async fn seed_session(pool: &PgPool, session_uuid: uuid::Uuid) -> anyhow::Result<()> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(format!("/proj/mailbox-staleness-{session_uuid}"))
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(session_uuid)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn boot_proxy_with_delivery_ttl(
    database_url: &str,
    mailbox_stale_minutes: i64,
    mailbox_delivery_ttl_minutes: i64,
) -> anyhow::Result<(String, CancellationToken)> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!(
            "persist_enabled = true\ndatabase_url = \"{database_url}\"\nmailbox_stale_minutes = {mailbox_stale_minutes}\nmailbox_delivery_ttl_minutes = {mailbox_delivery_ttl_minutes}\n"
        ),
    )?;

    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("mailbox staleness e2e proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = "http://127.0.0.1:1".to_string();

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr: SocketAddr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });
    tokio::task::yield_now().await;

    Ok((format!("http://{inspect_addr}"), shutdown))
}

pub(super) async fn seed_request(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
    turn_origin_code: &str,
    minutes_ago: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            request_body, created_at
         )
         SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 0, false,
                rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                convert_to('', 'UTF8'), now() - ($3 || ' minutes')::interval
           FROM sessions.sessions s,
                requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = $2",
    )
    .bind(session_uuid)
    .bind(turn_origin_code)
    .bind(minutes_ago.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn queue_message(
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

pub(super) async fn get_messages(
    client: &reqwest::Client,
    inspect_url: &str,
    session: uuid::Uuid,
) -> anyhow::Result<Value> {
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session}/messages"))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "get_session_messages returned {}",
        resp.status()
    );
    Ok(resp.json().await?)
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
