use crate::db_helpers;

use mike::app::inspect_router_on;
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

const KEY: &str = "steerer.verifier.input_budget";

async fn start_inspect(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(String, CancellationToken)> {
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
        database_url,
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let app = inspect_router_on(state, listener.local_addr()?);
    let serve_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { serve_shutdown.cancelled().await })
            .await
            .ok();
    });
    Ok((format!("http://{addr}"), shutdown))
}

async fn catalogue_entry(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<serde_json::Value> {
    let body: serde_json::Value = client
        .get(format!("{inspect_url}/api/settings"))
        .send()
        .await?
        .json()
        .await?;
    body["settings"]
        .as_array()
        .and_then(|all| all.iter().find(|s| s["key"] == KEY))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{KEY} missing from GET /api/settings: {body}"))
}

async fn patch_value(
    client: &reqwest::Client,
    inspect_url: &str,
    value: &str,
) -> anyhow::Result<reqwest::StatusCode> {
    let resp = client
        .patch(format!("{inspect_url}/api/settings/{KEY}"))
        .json(&serde_json::json!({ "value": value }))
        .send()
        .await?;
    Ok(resp.status())
}

#[tokio::test]
async fn verifier_input_budget_setting_listed_in_steerer_group_default_v1() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let upstream = MockServer::start().await;
    let (inspect_url, shutdown) = start_inspect(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let entry = catalogue_entry(&client, &inspect_url).await?;
    assert_eq!(entry["group"], "Steerer");
    assert_eq!(entry["label"], "Verifier input budget");
    assert_eq!(entry["default"], "verifier-input-v1");
    assert_eq!(
        entry["enum_variants"],
        serde_json::json!(["uncapped", "verifier-input-v1", "verifier-input-v1-wide"])
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_verifier_input_budget_accepts_each_code_and_rejects_unknown() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let upstream = MockServer::start().await;
    let (inspect_url, shutdown) = start_inspect(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    for code in ["uncapped", "verifier-input-v1-wide", "verifier-input-v1"] {
        let status = patch_value(&client, &inspect_url, code).await?;
        assert!(status.is_success(), "PATCH {code} returned {status}");
    }
    let rejected = patch_value(&client, &inspect_url, "half").await?;
    assert!(
        rejected.is_client_error(),
        "an unknown code must be rejected, got {rejected}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
