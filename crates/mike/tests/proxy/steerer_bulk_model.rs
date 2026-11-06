use crate::db_helpers;

use db_helpers::{TestDb, TestDbHandle};
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct ProxyWithDbAndOpenRouter {
    inspect_url: String,
    shutdown: CancellationToken,
    test_db: TestDbHandle,
}

async fn start_proxy_with_db_and_openrouter(
    upstream_url: &str,
    or_base_url: &str,
) -> anyhow::Result<Option<ProxyWithDbAndOpenRouter>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;

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
        &test_db.database_url,
    ])?;
    config.set_openrouter_base_url(or_base_url.to_owned());

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

    Ok(Some(ProxyWithDbAndOpenRouter {
        inspect_url: format!("http://{inspect_addr}"),
        shutdown,
        test_db,
    }))
}

const STRUCTURED_MODEL: &str = "openai/gpt-5.6-luna";
const NON_STRUCTURED_MODEL: &str = "meta-llama/llama-3.1-8b-instruct:free";

async fn mount_models_catalogue(or_server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": STRUCTURED_MODEL,
                    "name": "GPT 5.6 Luna",
                    "created": 1_780_000_000,
                    "supported_parameters": ["structured_outputs", "tools"],
                },
                {
                    "id": NON_STRUCTURED_MODEL,
                    "name": "Llama 3.1 8B Instruct (free)",
                    "created": 1_780_000_000,
                    "supported_parameters": ["tools"],
                },
            ]
        })))
        .mount(or_server)
        .await;
}

async fn create_steerer(
    client: &reqwest::Client,
    inspect_url: &str,
    name: &str,
    model: &str,
) -> anyhow::Result<i64> {
    let resp = client
        .post(format!("{inspect_url}/api/steerers"))
        .json(&serde_json::json!({
            "name": name,
            "display_name": name,
            "authority": "command",
            "mode": "drive",
            "model": model,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == reqwest::StatusCode::CREATED,
        "create_steerer({name}) expected 201, got {}: {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
    let body: Value = resp.json().await?;
    body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("steerer.id must be a number, got {body}"))
}

const CONFIGURED_STEERER_MODEL_DEFAULT: &str = "openai/gpt-5.6-luna";

#[tokio::test]
async fn bulk_set_all_models_updates_every_row_and_reports_count() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;
    mount_models_catalogue(&or_server).await;
    let upstream = MockServer::start().await;

    let Some(env) = start_proxy_with_db_and_openrouter(&upstream.uri(), &or_server.uri()).await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping bulk_set_all_models_updates_every_row_and_reports_count"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    create_steerer(
        &client,
        &env.inspect_url,
        &format!("bulk-model-a-{run_id}"),
        "some-original-model",
    )
    .await?;
    create_steerer(
        &client,
        &env.inspect_url,
        &format!("bulk-model-b-{run_id}"),
        "",
    )
    .await?;

    let before: Value = client
        .get(format!("{}/api/steerers", env.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let total_before = before
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GET /api/steerers must return an array"))?
        .len();
    anyhow::ensure!(
        total_before >= 2,
        "expected at least the 2 steerers just created, got {total_before}"
    );

    let put_resp = client
        .put(format!("{}/api/steerers/model", env.inspect_url))
        .json(&serde_json::json!({"model": STRUCTURED_MODEL}))
        .send()
        .await?;
    assert_eq!(
        put_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200: {}",
        put_resp.text().await.unwrap_or_default()
    );
    let put_body: Value = put_resp.json().await?;
    assert_eq!(
        put_body["count"].as_i64(),
        Some(total_before as i64),
        "count must equal every existing steerer row, including pre-seeded ones: {put_body}"
    );

    let after: Value = client
        .get(format!("{}/api/steerers", env.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let rows = after
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GET /api/steerers must return an array"))?;
    assert_eq!(rows.len(), total_before, "row count must be unchanged");
    for row in rows {
        assert_eq!(
            row["model"], STRUCTURED_MODEL,
            "every row's model must be updated: {row}"
        );
        assert_eq!(
            row["effective_model"], STRUCTURED_MODEL,
            "effective_model must equal the new override: {row}"
        );
        assert_eq!(
            row["model_inherited"], false,
            "an explicit model means not inherited: {row}"
        );
    }

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn bulk_set_all_models_rejects_non_structured_output_model_and_changes_nothing()
-> anyhow::Result<()> {
    let or_server = MockServer::start().await;
    mount_models_catalogue(&or_server).await;
    let upstream = MockServer::start().await;

    let Some(env) = start_proxy_with_db_and_openrouter(&upstream.uri(), &or_server.uri()).await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping bulk_set_all_models_rejects_non_structured_output_model_and_changes_nothing"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let steerer_id = create_steerer(
        &client,
        &env.inspect_url,
        &format!("bulk-model-reject-{run_id}"),
        "original-model",
    )
    .await?;

    let put_resp = client
        .put(format!("{}/api/steerers/model", env.inspect_url))
        .json(&serde_json::json!({"model": NON_STRUCTURED_MODEL}))
        .send()
        .await?;
    assert_eq!(
        put_resp.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "a non-structured-output model must be rejected 422"
    );
    let put_body: Value = put_resp.json().await?;
    assert!(
        put_body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "422 body must carry a clear error message: {put_body}"
    );

    let get_resp: Value = client
        .get(format!("{}/api/steerers/{steerer_id}", env.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        get_resp["model"], "original-model",
        "a rejected bulk apply must change nothing: {get_resp}"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn bulk_set_all_models_empty_string_resets_to_inherit() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;
    let upstream = MockServer::start().await;

    let Some(env) = start_proxy_with_db_and_openrouter(&upstream.uri(), &or_server.uri()).await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping bulk_set_all_models_empty_string_resets_to_inherit"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    create_steerer(
        &client,
        &env.inspect_url,
        &format!("bulk-model-reset-a-{run_id}"),
        "explicit-model-a",
    )
    .await?;
    create_steerer(
        &client,
        &env.inspect_url,
        &format!("bulk-model-reset-b-{run_id}"),
        "explicit-model-b",
    )
    .await?;

    let put_resp = client
        .put(format!("{}/api/steerers/model", env.inspect_url))
        .json(&serde_json::json!({"model": ""}))
        .send()
        .await?;
    assert_eq!(
        put_resp.status(),
        reqwest::StatusCode::OK,
        "empty-string reset must succeed without probing the catalogue: {}",
        put_resp.text().await.unwrap_or_default()
    );
    let put_body: Value = put_resp.json().await?;
    assert!(
        put_body["count"].as_i64().is_some_and(|c| c >= 2),
        "count must include at least the 2 steerers just created: {put_body}"
    );

    let after: Value = client
        .get(format!("{}/api/steerers", env.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let rows = after
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GET /api/steerers must return an array"))?;
    for row in rows {
        assert_eq!(row["model"], "", "every row's model must be cleared: {row}");
        assert_eq!(
            row["model_inherited"], true,
            "an empty model means inherited: {row}"
        );
        assert_eq!(
            row["effective_model"], CONFIGURED_STEERER_MODEL_DEFAULT,
            "effective_model must resolve to the configured steerer.model default: {row}"
        );
    }

    or_server.verify().await;
    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn list_steerers_reports_inherited_vs_overridden_for_mixed_set() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;
    let upstream = MockServer::start().await;

    let Some(env) = start_proxy_with_db_and_openrouter(&upstream.uri(), &or_server.uri()).await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping list_steerers_reports_inherited_vs_overridden_for_mixed_set"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let overridden_id = create_steerer(
        &client,
        &env.inspect_url,
        &format!("mixed-override-{run_id}"),
        STRUCTURED_MODEL,
    )
    .await?;
    let inherited_id = create_steerer(
        &client,
        &env.inspect_url,
        &format!("mixed-inherited-{run_id}"),
        "",
    )
    .await?;

    let list: Value = client
        .get(format!("{}/api/steerers", env.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let rows = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GET /api/steerers must return an array"))?;

    let overridden_row = rows
        .iter()
        .find(|r| r["id"].as_i64() == Some(overridden_id))
        .ok_or_else(|| anyhow::anyhow!("overridden steerer missing from list: {list}"))?;
    assert_eq!(overridden_row["model"], STRUCTURED_MODEL);
    assert_eq!(overridden_row["effective_model"], STRUCTURED_MODEL);
    assert_eq!(overridden_row["model_inherited"], false);

    let inherited_row = rows
        .iter()
        .find(|r| r["id"].as_i64() == Some(inherited_id))
        .ok_or_else(|| anyhow::anyhow!("inherited steerer missing from list: {list}"))?;
    assert_eq!(inherited_row["model"], "");
    assert_eq!(
        inherited_row["effective_model"],
        CONFIGURED_STEERER_MODEL_DEFAULT
    );
    assert_eq!(inherited_row["model_inherited"], true);

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
