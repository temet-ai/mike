use crate::anthropic_fixtures;
use crate::db_cap_helpers;

use std::collections::HashSet;

use anthropic_fixtures::{MockResponse, RequestBody};
use db_cap_helpers::{TestDb, TestDbHandle};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FALLBACK_OMITTED_FIELDS: &[&str] = &["ttl_override", "pending_inbox_count"];

async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .max_tokens(4)
        .content(user_text)
        .build();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == 200,
        "expected 200 OK, got {}",
        response.status()
    );
    let _ = response.bytes().await?;
    Ok(())
}

struct CappedProxy {
    proxy_url: String,
    inspect_url: String,
    shutdown: tokio_util::sync::CancellationToken,
    test_db: TestDbHandle,
}

async fn boot_small_cap_proxy(mock_uri: &str) -> anyhow::Result<Option<CappedProxy>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let (proxy_url, inspect_url, shutdown) =
        db_cap_helpers::start_proxy_with_db_at_capped(mock_uri, &test_db.database_url, 1).await?;

    Ok(Some(CappedProxy {
        proxy_url,
        inspect_url,
        shutdown,
        test_db,
    }))
}

async fn fetch_session_summary_json(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
) -> anyhow::Result<serde_json::Map<String, Value>> {
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session_id}"))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "GET /api/sessions/{session_id} expected 200, got {}",
        resp.status()
    );
    let body: Value = resp.json().await?;
    let summary = body
        .get("summary")
        .ok_or_else(|| anyhow::anyhow!("response missing `summary` envelope"))?;
    summary
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("summary must be a JSON object"))
}

#[tokio::test]
async fn session_summary_parity_live_vs_db_fallback_via_api() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_parity").usage(1, 1).build())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(capped) = boot_small_cap_proxy(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };

    let session_one = uuid::Uuid::new_v4().to_string();
    let session_two = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_request(&client, &capped.proxy_url, &session_one, "parity-first").await?;

    let live_summary =
        fetch_session_summary_json(&client, &capped.inspect_url, &session_one).await?;

    send_request(&client, &capped.proxy_url, &session_two, "parity-evict").await?;

    let fallback_summary =
        fetch_session_summary_json(&client, &capped.inspect_url, &session_one).await?;

    let live_keys: HashSet<&str> = live_summary.keys().map(String::as_str).collect();
    let fallback_keys: HashSet<&str> = fallback_summary.keys().map(String::as_str).collect();
    assert_eq!(
        live_keys, fallback_keys,
        "live and DB-fallback projections must produce the same JSON key set; \
         got live={live_keys:?} fallback={fallback_keys:?}"
    );

    let omitted: HashSet<&str> = FALLBACK_OMITTED_FIELDS.iter().copied().collect();

    for key in live_keys.iter().copied() {
        if omitted.contains(key) {
            continue;
        }
        assert_eq!(
            live_summary.get(key),
            fallback_summary.get(key),
            "parity mismatch on `{key}`: live={:?} fallback={:?}",
            live_summary.get(key),
            fallback_summary.get(key),
        );
    }

    for key in FALLBACK_OMITTED_FIELDS {
        assert!(
            fallback_summary.contains_key(*key),
            "omitted field `{key}` must still be present in DB-fallback JSON \
             (as default), not vanish"
        );
        assert!(
            live_summary.contains_key(*key),
            "omitted field `{key}` must still be present in live JSON"
        );
    }

    assert_eq!(
        fallback_summary
            .get("request_count")
            .and_then(Value::as_u64),
        Some(1),
        "DB-fallback must report request_count = 1, not 0"
    );
    assert_eq!(
        fallback_summary.get("last_status").and_then(Value::as_str),
        Some("Complete"),
        "DB-fallback must report last_status = Complete, not empty string"
    );
    let models = fallback_summary
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("models must be a JSON array"))?;
    assert_eq!(models.len(), 1, "models must carry the persisted model");
    assert_eq!(
        models[0].as_str(),
        Some("claude-sonnet-4-20250514"),
        "models[0] must equal the model the request used"
    );

    capped.shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    capped.test_db.cleanup().await?;
    Ok(())
}
