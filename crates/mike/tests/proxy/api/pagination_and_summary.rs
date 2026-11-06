use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_list_requests_pagination() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_page").text("Ok").usage(1, 1).build())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    for _ in 0..3 {
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-api-key", "sk-test")
            .header("content-type", "application/json")
            .header(
                "x-claude-code-session-id",
                "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            )
            .json(&RequestBody::default().content("Hi").max_tokens(1).build())
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let all_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    let all_body: Value = all_resp.json().await?;
    let all = all_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert_eq!(all.len(), 3);

    let limited_resp = client
        .get(format!("{inspect_url}/api/requests?limit=2"))
        .send()
        .await?;
    let limited_body: Value = limited_resp.json().await?;
    let limited = limited_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert_eq!(limited.len(), 2);

    let paged_resp = client
        .get(format!("{inspect_url}/api/requests?offset=1&limit=2"))
        .send()
        .await?;
    let paged_body: Value = paged_resp.json().await?;
    let paged = paged_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert_eq!(paged.len(), 2);

    let empty_resp = client
        .get(format!("{inspect_url}/api/requests?offset=100"))
        .send()
        .await?;
    let empty_body: Value = empty_resp.json().await?;
    let empty = empty_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected array"))?;
    assert!(empty.is_empty());

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_summary_endpoint_returns_single_row() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_summary_endpoint_case")
                        .text("Ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        )
        .json(&RequestBody::default().content("Hi").max_tokens(1).build())
        .send()
        .await?
        .error_for_status()?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let list_row = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let list: Value = client
                .get(format!("{inspect_url}/api/requests"))
                .send()
                .await?
                .json()
                .await?;
            if let Some(row) = list.as_array().and_then(|arr| arr.first()) {
                break row.clone();
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!("no request in list within 5s; last list={list}");
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };
    let id = list_row["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("list row has no u64 id"))?;

    let summary: Value = client
        .get(format!("{inspect_url}/api/requests/{id}/summary"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        summary, list_row,
        "single /summary row must match the list projection exactly"
    );

    for required in ["id", "session_id", "method", "path", "status"] {
        assert!(
            summary.get(required).is_some(),
            "summary must expose `{required}`: {summary}"
        );
    }
    assert!(
        summary.get("conversation_id").is_none(),
        "summary must NOT expose `conversation_id` (dropped in the session-drift closure): {summary}"
    );
    for forbidden in [
        "request_body_size",
        "request_body_truncated",
        "accumulated_text",
        "request_headers",
        "response_headers",
    ] {
        assert!(
            summary.get(forbidden).is_none(),
            "summary must not expose `{forbidden}`: {summary}"
        );
    }

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_summary_endpoint_unknown_id_returns_404() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests/99999/summary"))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_body_endpoints_unknown_id_returns_404() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    for suffix in ["body/request", "body/response"] {
        let resp = client
            .get(format!("{inspect_url}/api/requests/99999/{suffix}"))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            404,
            "unknown id must 404 on /{suffix}, got {}",
            resp.status()
        );
    }

    test_db.cleanup().await?;
    Ok(())
}
