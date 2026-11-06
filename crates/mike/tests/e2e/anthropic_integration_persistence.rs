use crate::db_helpers;

use mike::app::inspect_router_on;
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Inspect {
    url: String,
    shutdown: CancellationToken,
    service_token: String,
}

async fn start_inspect(upstream_url: &str, database_url: &str) -> anyhow::Result<Inspect> {
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
    let service_token = state.service_token().expose().to_owned();

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

    Ok(Inspect {
        url: format!("http://{inspect_addr}"),
        shutdown,
        service_token,
    })
}

const PUT_KEY: &str = "sk-ant-put-applied-secret";

async fn mount_models_ok(anthropic: &MockServer, key: &str) {
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", key))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .mount(anthropic)
        .await;
}

async fn set_mode(client: &reqwest::Client, inspect_url: &str, mode: &str) -> anyhow::Result<()> {
    let resp = client
        .patch(format!(
            "{inspect_url}/api/settings/assistant.anthropic_auth_mode"
        ))
        .json(&serde_json::json!({"value": mode}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    Ok(())
}

async fn count_settings_rows_holding(database_url: &str, needle: &str) -> anyhow::Result<i64> {
    let pool = sqlx::PgPool::connect(database_url).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM config.settings WHERE value::text LIKE $1")
            .bind(format!("%{needle}%"))
            .fetch_one(&pool)
            .await?;
    Ok(count)
}

#[tokio::test]
async fn put_key_applies_in_memory_persists_nothing_mode_persists_across_restart()
-> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let anthropic = MockServer::start().await;
    mount_models_ok(&anthropic, PUT_KEY).await;

    let Inspect {
        url: inspect_url,
        shutdown,
        service_token,
    } = start_inspect(&anthropic.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    set_mode(&client, &inspect_url, "api_key").await?;

    let put_resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": PUT_KEY}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);
    let put_body: serde_json::Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], true);
    assert_eq!(
        put_body["persisted"], false,
        "the key must never be reported as durably stored: {put_body}"
    );

    assert_eq!(
        count_settings_rows_holding(&test_db.database_url, "put-applied-secret").await?,
        0,
        "no config.settings row may ever hold the raw Anthropic key"
    );

    let creds: serde_json::Value = client
        .get(format!(
            "{inspect_url}/api/settings/assistant/anthropic/credentials"
        ))
        .header(mike::service_token::HEADER, &service_token)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(creds["mode"], "api_key");
    assert_eq!(
        creds["api_key"], PUT_KEY,
        "the service-only credentials route must serve this process's key: {creds}"
    );

    shutdown.cancel();

    let Inspect {
        url: inspect_url_2,
        shutdown: shutdown_2,
        service_token: service_token_2,
    } = start_inspect(&anthropic.uri(), &test_db.database_url).await?;
    let creds_after_restart: serde_json::Value = client
        .get(format!(
            "{inspect_url_2}/api/settings/assistant/anthropic/credentials"
        ))
        .header(mike::service_token::HEADER, &service_token_2)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        creds_after_restart["mode"], "api_key",
        "the auth-mode setting must survive a restart: {creds_after_restart}"
    );
    assert!(
        creds_after_restart["api_key"].is_null(),
        "the API key itself must never survive a restart: {creds_after_restart}"
    );

    shutdown_2.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn credentials_gate_the_key_by_the_persisted_mode() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let anthropic = MockServer::start().await;
    mount_models_ok(&anthropic, "sk-ant-gated-key").await;

    let Inspect {
        url: inspect_url,
        shutdown,
        service_token,
    } = start_inspect(&anthropic.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let put_resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": "sk-ant-gated-key"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);

    let creds_before: serde_json::Value = client
        .get(format!(
            "{inspect_url}/api/settings/assistant/anthropic/credentials"
        ))
        .header(mike::service_token::HEADER, &service_token)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(creds_before["mode"], "claude_code_login");
    assert!(
        creds_before["api_key"].is_null(),
        "a key sitting in the slot must not leak while mode is claude_code_login: {creds_before}"
    );

    set_mode(&client, &inspect_url, "api_key").await?;

    let creds_after: serde_json::Value = client
        .get(format!(
            "{inspect_url}/api/settings/assistant/anthropic/credentials"
        ))
        .header(mike::service_token::HEADER, &service_token)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(creds_after["mode"], "api_key");
    assert_eq!(creds_after["api_key"], "sk-ant-gated-key");

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
