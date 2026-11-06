use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn post_turns_and_discover_session_id(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    session_uuid: &str,
    count: usize,
) -> anyhow::Result<String> {
    for _ in 0..count {
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-api-key", "sk-test")
            .header("content-type", "application/json")
            .header("X-Claude-Code-Session-Id", session_uuid)
            .json(
                &RequestBody::default()
                    .content("Shared first message")
                    .max_tokens(1)
                    .build(),
            )
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let list: Value = client
        .get(format!("{inspect_url}/api/sessions"))
        .send()
        .await?
        .json()
        .await?;
    let rows = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("list must be an array"))?;
    assert_eq!(rows.len(), 1, "exactly one session expected");
    rows[0]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("session id must be a string"))
        .map(String::from)
}

async fn assert_session_detail_chronological_and_shape(
    client: &reqwest::Client,
    inspect_url: &str,
    sid: &str,
) -> anyhow::Result<()> {
    let detail_resp = client
        .get(format!("{inspect_url}/api/sessions/{sid}?order=asc"))
        .send()
        .await?;
    assert_eq!(detail_resp.status(), 200);
    let detail: Value = detail_resp.json().await?;

    assert!(detail.get("summary").is_some(), "detail missing summary");
    let requests = detail
        .get("requests")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("detail.requests must be an array"))?;
    assert_eq!(
        requests.len(),
        3,
        "all three turns must appear in the detail payload"
    );

    let summary = detail
        .get("summary")
        .ok_or_else(|| anyhow::anyhow!("summary must be present"))?;
    assert_eq!(summary["id"], Value::String(sid.to_string()));
    assert_eq!(summary["request_count"], Value::from(3u64));

    let ids: Vec<u64> = requests.iter().filter_map(|r| r["id"].as_u64()).collect();
    assert_eq!(ids.len(), 3);
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "requests in session detail must be in chronological ascending order (order=asc)"
    );

    for row in requests {
        for forbidden in [
            "accumulated_text",
            "request_body_size",
            "response_body_size",
            "request_headers",
            "response_headers",
            "conversation_id",
        ] {
            assert!(
                row.get(forbidden).is_none(),
                "session detail row must not expose `{forbidden}`: {row}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_get_session_404_unknown_and_sorted_known() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_sess_detail")
                        .text("ok")
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

    let unknown_resp = client
        .get(format!("{inspect_url}/api/sessions/does-not-exist"))
        .send()
        .await?;
    assert_eq!(
        unknown_resp.status(),
        404,
        "unknown session id must 404 via StatusCode::NOT_FOUND"
    );

    let session_uuid = "11111111-1111-4111-8111-111111111111";
    let sid =
        post_turns_and_discover_session_id(&client, &proxy_url, &inspect_url, session_uuid, 3)
            .await?;
    assert_session_detail_chronological_and_shape(&client, &inspect_url, &sid).await?;

    test_db.cleanup().await?;
    Ok(())
}

async fn post_turns_and_get_session_id(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    session_uuid: &str,
    count: usize,
) -> anyhow::Result<String> {
    for _ in 0..count {
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-api-key", "sk-test")
            .header("content-type", "application/json")
            .header("X-Claude-Code-Session-Id", session_uuid)
            .json(
                &RequestBody::default()
                    .content("Same first message")
                    .max_tokens(1)
                    .build(),
            )
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let list: Value = client
        .get(format!("{inspect_url}/api/sessions"))
        .send()
        .await?
        .json()
        .await?;
    list.as_array()
        .and_then(|arr| arr.first())
        .and_then(|row| row["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("expected at least one session row"))
        .map(String::from)
}

async fn assert_legacy_shape_when_no_params(
    client: &reqwest::Client,
    inspect_url: &str,
    sid: &str,
) -> anyhow::Result<()> {
    let full: Value = client
        .get(format!("{inspect_url}/api/sessions/{sid}"))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        full.get("total_requests").is_none(),
        "legacy response shape must NOT include total_requests"
    );
    assert!(full.get("offset").is_none());
    assert!(full.get("limit").is_none());
    let full_requests = full["requests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert_eq!(full_requests.len(), 5);
    Ok(())
}

async fn assert_paginated_shape_with_offset_and_limit(
    client: &reqwest::Client,
    inspect_url: &str,
    sid: &str,
) -> anyhow::Result<()> {
    let paged: Value = client
        .get(format!("{inspect_url}/api/sessions/{sid}?offset=2&limit=2"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(paged["total_requests"], 5);
    assert_eq!(paged["offset"], 2);
    assert_eq!(paged["limit"], 2);
    let paged_requests = paged["requests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert_eq!(paged_requests.len(), 2);
    assert_eq!(paged["summary"]["request_count"], 5);
    Ok(())
}

async fn assert_limit_clamps_to_max(
    client: &reqwest::Client,
    inspect_url: &str,
    sid: &str,
) -> anyhow::Result<()> {
    let big: Value = client
        .get(format!("{inspect_url}/api/sessions/{sid}?limit=9999"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(big["total_requests"], 5);
    assert_eq!(big["offset"], 0, "missing offset must default to 0");
    assert_eq!(
        big["limit"], 500,
        "limit must clamp to SESSION_DETAIL_MAX_LIMIT"
    );
    let big_requests = big["requests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert_eq!(big_requests.len(), 5);
    Ok(())
}

async fn assert_offset_past_end_is_empty(
    client: &reqwest::Client,
    inspect_url: &str,
    sid: &str,
) -> anyhow::Result<()> {
    let past: Value = client
        .get(format!("{inspect_url}/api/sessions/{sid}?offset=100"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(past["total_requests"], 5);
    assert_eq!(past["offset"], 100);
    assert_eq!(past["limit"], 50, "missing limit must default to 50");
    let past_requests = past["requests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert_eq!(past_requests.len(), 0);
    Ok(())
}

#[tokio::test]
async fn test_get_session_with_offset_and_limit_paginates() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_sess_page")
                        .text("ok")
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

    let session_uuid = "22222222-2222-4222-8222-222222222222";
    let sid =
        post_turns_and_get_session_id(&client, &proxy_url, &inspect_url, session_uuid, 5).await?;

    assert_legacy_shape_when_no_params(&client, &inspect_url, &sid).await?;
    assert_paginated_shape_with_offset_and_limit(&client, &inspect_url, &sid).await?;
    assert_limit_clamps_to_max(&client, &inspect_url, &sid).await?;
    assert_offset_past_end_is_empty(&client, &inspect_url, &sid).await?;

    test_db.cleanup().await?;
    Ok(())
}
