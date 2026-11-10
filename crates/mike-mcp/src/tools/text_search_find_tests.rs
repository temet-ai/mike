use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

fn args(cursor: Option<u64>) -> anyhow::Result<FindTextArgs> {
    let mut value = json!({"needle": "needle-x", "session_id": SESSION});
    if let Some(cursor) = cursor {
        value["cursor"] = json!(cursor);
    }
    Ok(serde_json::from_value(value)?)
}

fn empty_incomplete(cursor: u64) -> Value {
    json!({
        "matches": [], "scan_complete": false, "has_more": true,
        "next_cursor": cursor, "requests_scanned": 40, "requests_total": 900,
        "full_match_count": null
    })
}

async fn mount(server: &MockServer, cursor: Option<u64>, body: Value) {
    let builder = Mock::given(method("GET")).and(path("/api/body/find"));
    let builder = match cursor {
        Some(cursor) => builder.and(query_param("cursor", cursor.to_string())),
        None => builder,
    };
    builder
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn find_text_settled_empty_incomplete_answer_is_followed_by_a_cursor_call()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    let found = json!({
        "matches": [{"request_id": 77}], "scan_complete": true, "has_more": false,
        "full_match_count": 1, "requests_scanned": 900, "requests_total": 900
    });
    mount(&server, Some(500), found.clone()).await;
    mount(&server, None, empty_incomplete(500)).await;
    let client = InspectClient::new(server.uri());

    let value = find_text_settled(&client, args(None)?, FIND_TEXT_SETTLE_BUDGET).await?;

    assert_eq!(value, found);
    let requests = server.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(
        requests[1]
            .url
            .query_pairs()
            .any(|(k, v)| k == "cursor" && v == "500"),
        "the second call must carry the first answer's next_cursor"
    );
    Ok(())
}

#[tokio::test]
async fn find_text_settled_stays_empty_and_incomplete_returns_tool_error_naming_the_cursor()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    mount(&server, None, empty_incomplete(500)).await;
    let client = InspectClient::new(server.uri());

    let outcome = find_text_settled(&client, args(None)?, Duration::ZERO).await;

    let error = outcome
        .err()
        .ok_or_else(|| anyhow::anyhow!("an empty incomplete scan was returned as an answer"))?;
    assert!(error.message.contains("40 of 900 requests"), "{error:?}");
    assert!(error.message.contains("cursor=500"), "{error:?}");
    let data = error.data.ok_or_else(|| anyhow::anyhow!("no data"))?;
    assert_eq!(data["next_cursor"], json!(500));
    Ok(())
}

#[tokio::test]
async fn find_text_settled_repeated_cursor_stops_instead_of_looping() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    mount(&server, Some(500), empty_incomplete(500)).await;
    let client = InspectClient::new(server.uri());

    let outcome = find_text_settled(&client, args(Some(500))?, FIND_TEXT_SETTLE_BUDGET).await;

    assert!(outcome.is_err());
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn find_text_settled_empty_complete_answer_is_returned_as_is() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let absent = json!({
        "matches": [], "scan_complete": true, "has_more": false,
        "full_match_count": 0, "requests_scanned": 900, "requests_total": 900
    });
    mount(&server, None, absent.clone()).await;
    let client = InspectClient::new(server.uri());

    let value = find_text_settled(&client, args(None)?, FIND_TEXT_SETTLE_BUDGET).await?;

    assert_eq!(value, absent);
    Ok(())
}

#[tokio::test]
async fn find_text_settled_server_error_is_passed_through() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/body/find"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "bad"})))
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());

    let outcome = find_text_settled(&client, args(None)?, FIND_TEXT_SETTLE_BUDGET).await;

    assert!(outcome.is_err());
    Ok(())
}
