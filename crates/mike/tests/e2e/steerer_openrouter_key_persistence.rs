use crate::db_helpers;

use mike::app::inspect_router_on;
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn start_inspect(
    upstream_url: &str,
    database_url: &str,
    or_base_url: &str,
    boot_config_key: Option<&str>,
) -> anyhow::Result<(String, CancellationToken)> {
    let mut config = ProxyConfig::try_parse_from([
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
    config.set_openrouter_base_url(or_base_url.to_owned());
    if let Some(key) = boot_config_key {
        config.set_openrouter_api_key(key.to_owned());
    }

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    Ok((format!("http://{inspect_addr}"), shutdown))
}

const PUT_KEY: &str = "sk-or-v1-put-applied-secret";
const LEGACY_DB_KEY: &str = "sk-or-v1-legacy-db-secret";
const ENV_KEY: &str = "sk-or-v1-env-configured-key";
const ENV_KEY_LABEL: &str = "sk-or-v1-env-conf...key";
const PUT_KEY_LABEL: &str = "sk-or-v1-put-appl...ret";

async fn mount_key_probe(or_server: &MockServer, key: &str, label: &str) {
    Mock::given(method("GET"))
        .and(path("/key"))
        .and(header("authorization", format!("Bearer {key}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"label": label, "usage": 0.0, "is_free_tier": false}
        })))
        .mount(or_server)
        .await;
}

async fn mount_key_credits(or_server: &MockServer, key: &str) {
    Mock::given(method("GET"))
        .and(path("/credits"))
        .and(header("authorization", format!("Bearer {key}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"total_credits": 5.0, "total_usage": 0.0}
        })))
        .mount(or_server)
        .await;
}

async fn count_settings_rows_holding(database_url: &str, needle: &str) -> anyhow::Result<i64> {
    let pool = sqlx::PgPool::connect(database_url).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM config.settings \
         WHERE key = 'steerer.openrouter_api_key' OR value::text LIKE $1",
    )
    .bind(format!("%{needle}%"))
    .fetch_one(&pool)
    .await?;
    Ok(count)
}

async fn put_key(client: &reqwest::Client, inspect_url: &str) -> anyhow::Result<serde_json::Value> {
    let resp = client
        .put(format!("{inspect_url}/api/settings/steerer/openrouter/key"))
        .json(&serde_json::json!({"key": PUT_KEY}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    Ok(resp.json().await?)
}

#[tokio::test]
async fn put_key_applies_in_memory_persists_nothing_stays_masked() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let or_server = MockServer::start().await;
    mount_key_probe(&or_server, PUT_KEY, PUT_KEY_LABEL).await;
    mount_key_credits(&or_server, PUT_KEY).await;
    let upstream = MockServer::start().await;

    let (inspect_url, shutdown) = start_inspect(
        &upstream.uri(),
        &test_db.database_url,
        &or_server.uri(),
        None,
    )
    .await?;
    let client = reqwest::Client::new();

    let put_body = put_key(&client, &inspect_url).await?;
    assert_eq!(put_body["valid"], true);
    assert_eq!(
        put_body["persisted"], false,
        "the key must never be reported as durably stored: {put_body}"
    );
    let label = put_body["label"].as_str().unwrap_or_default();
    assert!(
        !label.contains("put-applied-secret"),
        "PUT response label leaked the raw key: {label}"
    );

    let settings_text = client
        .get(format!("{inspect_url}/api/settings"))
        .send()
        .await?
        .text()
        .await?;
    assert!(
        !settings_text.contains("put-applied-secret"),
        "GET /api/settings must never expose the key: {settings_text}"
    );

    assert_eq!(
        count_settings_rows_holding(&test_db.database_url, "put-applied-secret").await?,
        0,
        "no config.settings row may hold the key or a steerer.openrouter_api_key row"
    );

    let usage: serde_json::Value = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(usage["configured"], true);
    assert_eq!(
        usage["label"], PUT_KEY_LABEL,
        "the applied key must serve this process: {usage}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn boot_ignores_legacy_db_key_row_and_uses_configured_key() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let or_server = MockServer::start().await;
    mount_key_probe(&or_server, ENV_KEY, ENV_KEY_LABEL).await;
    mount_key_credits(&or_server, ENV_KEY).await;
    let upstream = MockServer::start().await;

    let (_first_url, first_shutdown) = start_inspect(
        &upstream.uri(),
        &test_db.database_url,
        &or_server.uri(),
        None,
    )
    .await?;
    first_shutdown.cancel();

    let pool = sqlx::PgPool::connect(&test_db.database_url).await?;
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ('steerer.openrouter_api_key', $1)",
    )
    .bind(serde_json::json!(LEGACY_DB_KEY))
    .execute(&pool)
    .await?;

    let (inspect_url, shutdown) = start_inspect(
        &upstream.uri(),
        &test_db.database_url,
        &or_server.uri(),
        Some(ENV_KEY),
    )
    .await?;

    let client = reqwest::Client::new();
    let usage_resp = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?;
    assert_eq!(
        usage_resp.status(),
        reqwest::StatusCode::OK,
        "boot must resolve the configured key; a legacy DB row winning would send a bearer \
         token the mocks above do not accept and surface as a non-200"
    );
    let usage: serde_json::Value = usage_resp.json().await?;
    assert_eq!(usage["configured"], true);
    assert_eq!(
        usage["label"], ENV_KEY_LABEL,
        "boot must resolve the configured key, not a legacy settings row: {usage}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
