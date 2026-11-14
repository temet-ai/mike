use std::net::SocketAddr;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
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
) -> anyhow::Result<(AppState, String, CancellationToken)> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!(
            "persist_enabled = true\ndatabase_url = \"{database_url}\"\n\n[reminders]\nenabled = true\ninject_lessons = true\n\n[lesson_worker]\nenabled = false\n"
        ),
    )?;

    mike::feature_settings::install();
    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("lesson-injection proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = upstream_url.to_string();

    let state = AppState::new(config).await?;
    mike::lesson_delivery::hooks::install(&state);
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr: SocketAddr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
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

    Ok((state, format!("http://{proxy_addr}"), shutdown))
}

pub(super) fn ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(
            MockResponse::new("msg_lesson")
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

pub(super) fn body_with_working_dir(working_dir: &str, user_text: &str) -> Value {
    RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(format!(
            "You are Claude.\n\nWorking directory: {working_dir}\n"
        ))]))
        .content(json!([text_block(user_text)]))
        .build()
}

pub(super) async fn insert_lesson(
    pool: &PgPool,
    project_id: i64,
    kind: &str,
    text: &str,
    stage: &str,
) -> anyhow::Result<uuid::Uuid> {
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, $3, ls.id, lsrc.id
         FROM memory.lesson_kinds lk,
              memory.lesson_stages ls,
              memory.lesson_sources lsrc
         WHERE lk.code = $2
           AND ls.code = $4
           AND lsrc.code = 'operator'
         RETURNING uuid",
    )
    .bind(project_id)
    .bind(kind)
    .bind(text)
    .bind(stage)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn project_id_for_path(
    pool: &PgPool,
    project_path: &str,
) -> anyhow::Result<Option<i64>> {
    let id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(project_path)
            .fetch_optional(pool)
            .await?;
    Ok(id)
}

pub(super) async fn create_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1)
         ON CONFLICT (project_path) DO UPDATE SET display_name = EXCLUDED.display_name
         RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
