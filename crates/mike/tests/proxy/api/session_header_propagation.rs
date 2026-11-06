use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn post_with_session_header(
    client: &reqwest::Client,
    proxy_url: &str,
    session_uuid: &str,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header("X-Claude-Code-Session-Id", session_uuid)
        .json(
            &RequestBody::default()
                .content("with session")
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn post_without_session_header(
    client: &reqwest::Client,
    proxy_url: &str,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .content("without session")
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        200,
        "a header-less request must still be forwarded upstream"
    );
    let _ = resp.bytes().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn assert_only_headered_request_captured(
    client: &reqwest::Client,
    inspect_url: &str,
    session_uuid: &str,
) -> anyhow::Result<()> {
    let list: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let rows = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("list must be an array"))?;
    assert_eq!(
        rows.len(),
        1,
        "exactly one captured request expected (the header-less request is not captured)"
    );

    for row in rows {
        let sid = row["session_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("session_id must be a string on the list DTO"))?;
        assert_ne!(
            sid, "no-session",
            "the `no-session` marker must never be persisted — header-less requests are not captured"
        );
    }

    let with = &rows[0];
    assert_eq!(
        with["session_id"],
        Value::String(session_uuid.to_string()),
        "X-Claude-Code-Session-Id must round-trip to the list DTO"
    );

    let id = with["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be u64"))?;
    let detail: Value = client
        .get(format!("{inspect_url}/api/requests/{id}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        detail["session_id"],
        Value::String(session_uuid.to_string()),
        "X-Claude-Code-Session-Id must round-trip to the detail DTO"
    );
    Ok(())
}

#[tokio::test]
async fn session_id_header_propagates_to_request_detail_dto() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_session_test")
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

    let session_uuid = "550e8400-e29b-41d4-a716-446655440000";
    post_with_session_header(&client, &proxy_url, session_uuid).await?;
    post_without_session_header(&client, &proxy_url).await?;
    assert_only_headered_request_captured(&client, &inspect_url, session_uuid).await?;

    test_db.cleanup().await?;
    Ok(())
}
