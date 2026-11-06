use crate::anthropic_fixtures;

use anthropic_fixtures::MockResponse;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{
    boot_small_cap_proxy, send_request, wait_for_in_memory_eviction, wait_until_terminal,
};

async fn send_first_request_and_wait_for_id(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    session_one: &str,
) -> anyhow::Result<u64> {
    send_request(client, proxy_url, session_one, "first").await?;

    let list_before: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let arr = list_before
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    anyhow::ensure!(!arr.is_empty(), "first request must be visible pre-evict");
    let first_id = arr[0]
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("first.id must be u64"))?;

    wait_until_terminal(client, inspect_url, first_id).await?;
    Ok(first_id)
}

async fn evict_first_via_second_request(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    session_two: &str,
    first_id: u64,
) -> anyhow::Result<()> {
    send_request(client, proxy_url, session_two, "second").await?;

    let mem_snap = wait_for_in_memory_eviction(client, inspect_url, first_id).await?;
    let still_in_mem = mem_snap
        .get("top_entries")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("id").and_then(|v| v.as_u64()))
                .any(|id| id == first_id)
        })
        .unwrap_or(false);
    anyhow::ensure!(
        !still_in_mem,
        "first request must be evicted from in-memory store with cap=1 after the second arrives; \
         memory snapshot after wait = {mem_snap:?}"
    );
    Ok(())
}

async fn assert_detail_and_summary_fall_back(
    client: &reqwest::Client,
    inspect_url: &str,
    first_id: u64,
    session_one: &str,
) -> anyhow::Result<()> {
    let detail_resp = client
        .get(format!("{inspect_url}/api/requests/{first_id}"))
        .send()
        .await?;
    assert_eq!(
        detail_resp.status(),
        200,
        "DB fallback must serve evicted id as 200, not 404"
    );
    let detail: Value = detail_resp.json().await?;
    assert_eq!(
        detail
            .get("id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("detail.id missing"))?,
        first_id,
        "DB-fallback detail must echo the same id"
    );
    assert_eq!(
        detail.get("session_id").and_then(|v| v.as_str()),
        Some(session_one),
        "DB-fallback detail must echo the same session_id"
    );

    let summary_resp = client
        .get(format!("{inspect_url}/api/requests/{first_id}/summary"))
        .send()
        .await?;
    assert_eq!(summary_resp.status(), 200, "summary DB fallback must 200");
    let summary: Value = summary_resp.json().await?;
    assert_eq!(
        summary.get("id").and_then(|v| v.as_u64()),
        Some(first_id),
        "summary.id must match"
    );

    let missing_resp = client
        .get(format!("{inspect_url}/api/requests/{}", u64::MAX - 1))
        .send()
        .await?;
    assert_eq!(
        missing_resp.status(),
        404,
        "unknown id must still 404 after fallback lookup"
    );
    Ok(())
}

#[tokio::test]
async fn get_request_falls_back_to_db_after_eviction() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_db_fallback_req").usage(1, 1).build())
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

    let first_id = send_first_request_and_wait_for_id(
        &client,
        &capped.proxy_url,
        &capped.inspect_url,
        &session_one,
    )
    .await?;
    evict_first_via_second_request(
        &client,
        &capped.proxy_url,
        &capped.inspect_url,
        &session_two,
        first_id,
    )
    .await?;
    assert_detail_and_summary_fall_back(&client, &capped.inspect_url, first_id, &session_one)
        .await?;

    capped.shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    capped.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_request_body_falls_back_to_db_after_eviction() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_db_fallback_body")
                        .usage(1, 1)
                        .build(),
                )
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

    send_request(&client, &capped.proxy_url, &session_one, "body-test-first").await?;
    let list_before: Value = client
        .get(format!("{}/api/requests", capped.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let arr = list_before
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests array"))?;
    anyhow::ensure!(!arr.is_empty(), "first request must be visible");
    let first_id = arr[0]
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("first.id must be u64"))?;

    send_request(&client, &capped.proxy_url, &session_two, "second").await?;

    let body_resp = client
        .get(format!(
            "{}/api/requests/{first_id}/body/request",
            capped.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(
        body_resp.status(),
        200,
        "request body DB fallback must 200, got {}",
        body_resp.status()
    );
    let payload: Value = body_resp.json().await?;
    assert_eq!(
        payload.get("has_body").and_then(|v| v.as_bool()),
        Some(true),
        "evicted request with a body must still report has_body=true from DB"
    );

    let resp_body = client
        .get(format!(
            "{}/api/requests/{first_id}/body/response",
            capped.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(
        resp_body.status(),
        200,
        "response body DB fallback must 200"
    );

    let missing = client
        .get(format!(
            "{}/api/requests/{}/body/request",
            capped.inspect_url,
            u64::MAX - 1
        ))
        .send()
        .await?;
    assert_eq!(
        missing.status(),
        404,
        "unknown id must 404 even after fallback lookup"
    );

    capped.shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    capped.test_db.cleanup().await?;
    Ok(())
}
