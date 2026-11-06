use crate::db_helpers;

use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};

pub(super) async fn register(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: uuid::Uuid,
    cwd: &str,
    intent: Option<(&str, &str)>,
) -> anyhow::Result<(reqwest::StatusCode, Value)> {
    let mut body = json!({
        "session_id": session_id.to_string(),
        "cwd": cwd,
        "repo_root": cwd,
    });
    if let Some((goal, next_steps)) = intent {
        body["intent"] = json!(goal);
        body["next_steps"] = json!(next_steps);
    }
    let resp = client
        .post(format!("{inspect_url}/api/workspace/register"))
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let json: Value = resp.json().await?;
    Ok((status, json))
}

pub(super) async fn register_with_pid(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: uuid::Uuid,
    cwd: &str,
    pid: i32,
) -> anyhow::Result<(reqwest::StatusCode, Value)> {
    let body = json!({
        "session_id": session_id.to_string(),
        "cwd": cwd,
        "repo_root": cwd,
        "pid": pid,
    });
    let resp = client
        .post(format!("{inspect_url}/api/workspace/register"))
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let json: Value = resp.json().await?;
    Ok((status, json))
}

pub(super) async fn start_proxy_with_shared_db_and_state(
    upstream_url: &str,
) -> anyhow::Result<
    Option<(
        String,
        AppState,
        tokio_util::sync::CancellationToken,
        db_helpers::TestDbHandle,
    )>,
> {
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_shared().await?;
    boot_proxy_with_state(upstream_url, test_db).await
}

async fn boot_proxy_with_state(
    upstream_url: &str,
    test_db: db_helpers::TestDbHandle,
) -> anyhow::Result<
    Option<(
        String,
        AppState,
        tokio_util::sync::CancellationToken,
        db_helpers::TestDbHandle,
    )>,
> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &test_db.database_url,
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let inspect_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
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

    Ok(Some((
        format!("http://{inspect_addr}"),
        state,
        shutdown,
        test_db,
    )))
}
