use crate::common;

use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

struct StartedHealthProxy {
    inspect_url: String,
    _tmp: tempfile::TempDir,
    shutdown: CancellationToken,
}

impl Drop for StartedHealthProxy {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn start_proxy_with_embedding_enabled(
    upstream_url: &str,
) -> anyhow::Result<StartedHealthProxy> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        "persist_enabled = false\n\n[embedding]\nenabled = true\n",
    )?;

    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("health proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = upstream_url.to_string();

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
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

    Ok(StartedHealthProxy {
        inspect_url: format!("http://{inspect_addr}"),
        _tmp: tmp,
        shutdown,
    })
}

#[tokio::test]
async fn test_health_endpoint_degrades_when_embedding_armed_and_unreachable() -> anyhow::Result<()>
{
    let mock_server = MockServer::start().await;
    let proxy = start_proxy_with_embedding_enabled(&mock_server.uri()).await?;
    let inspect_url = proxy.inspect_url.clone();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(
        body["embedding"]["reachable"],
        json!(false),
        "no embedding service runs in this harness, so the probe must say so: {body}"
    );
    assert_eq!(
        body["status"],
        json!("degraded"),
        "an enabled-but-unreachable subsystem must degrade the aggregate: {body}"
    );

    Ok(())
}

const FIXED_IDENTITIES: [&str; 3] = ["unknown", "dev-build", "mbx-build"];

#[tokio::test]
async fn test_health_reports_build_identity() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let body: Value = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?
        .json()
        .await?;

    let build = &body["build"];
    let sha = build["sha"]
        .as_str()
        .unwrap_or_else(|| panic!("health must carry build.sha, got: {body}"));
    let short = build["sha_short"]
        .as_str()
        .unwrap_or_else(|| panic!("health must carry build.sha_short, got: {body}"));

    if !FIXED_IDENTITIES.contains(&sha) {
        assert_eq!(sha.len(), 40, "build.sha must be a full commit hash: {sha}");
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "build.sha must be hex: {sha}"
        );
        assert!(
            sha.starts_with(short),
            "build.sha_short ({short}) must abbreviate build.sha ({sha})"
        );
    } else {
        assert_eq!(short, sha, "both sha fields fall back together");
    }

    assert!(
        build["dirty"].is_boolean(),
        "health must carry build.dirty, got: {body}"
    );
    let built_at = build["built_at"]
        .as_str()
        .unwrap_or_else(|| panic!("health must carry build.built_at, got: {body}"));
    assert!(
        FIXED_IDENTITIES.contains(&built_at)
            || (built_at.len() == 20 && built_at.ends_with('Z') && built_at.contains('T')),
        "build.built_at must be RFC 3339 UTC: {built_at}"
    );

    assert!(
        body["status"].is_string(),
        "status must survive as a string: {body}"
    );
    assert!(
        body["embedding"].is_object(),
        "embedding must survive: {body}"
    );

    Ok(())
}

#[tokio::test]
async fn test_stats_endpoint_empty() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/stats"))
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["request_count"], 0);
    assert!(
        body.get("store_size").is_none(),
        "store_size must be absent — dropped in Phase D"
    );
    assert!(
        body.get("compose_enabled").is_none(),
        "compose_enabled must be absent — compose spawner removed"
    );
    assert_eq!(
        body["project_count"],
        Value::Null,
        "project_count must be null when persistence is disabled, got: {body}"
    );

    Ok(())
}
