use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::RequestBody;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_streaming_detail_exposes_headers_body_and_completion() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_sse_nulltest\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(sse_body.as_bytes(), "text/event-stream")
                .insert_header("anthropic-ratelimit-requests-limit", "1000"),
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

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        )
        .json(
            &RequestBody::default()
                .content("Hi streaming")
                .stream(true)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let id = poll_first_captured_request_id(&client, &inspect_url).await?;
    let detail = poll_detail_until_complete(&client, &inspect_url, id).await?;
    assert_completion_fields(&detail)?;

    assert_request_body_round_trips(&client, &inspect_url, id).await?;
    assert_assembled_response_body(&client, &inspect_url, id).await?;
    assert_detail_response_size_reflects_assembled_body(&client, &inspect_url, id).await?;
    assert_ratelimit_headers_present(&detail)?;

    test_db.cleanup().await?;
    Ok(())
}

async fn poll_first_captured_request_id(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<u64> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let list: Value = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?
            .json()
            .await?;
        if let Some(id) = list
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["id"].as_u64())
        {
            return Ok(id);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("no request id in list within 5s; last list={list}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn poll_detail_until_complete(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let d: Value = client
            .get(format!("{inspect_url}/api/requests/{id}"))
            .send()
            .await?
            .json()
            .await?;
        if d["status"] == "Complete" || std::time::Instant::now() > deadline {
            return Ok(d);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

fn assert_completion_fields(detail: &Value) -> anyhow::Result<()> {
    assert_eq!(detail["status"], "Complete", "stream must reach Complete");
    assert_eq!(detail["stop_reason"], Value::String("end_turn".to_string()));
    assert!(
        detail["completed_at"].is_string(),
        "completed_at must be set"
    );

    assert!(
        detail.get("request_body_text").is_none(),
        "detail must not expose request_body_text in new shape: {detail}"
    );
    assert!(
        detail.get("response_body_text").is_none(),
        "detail must not expose response_body_text in new shape: {detail}"
    );
    assert!(
        detail["request_body_size"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("request_body_size missing on streaming detail"))?
            > 0
    );
    Ok(())
}

async fn assert_request_body_round_trips(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<()> {
    let req_body: Value = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let r = client
                .get(format!("{inspect_url}/api/requests/{id}/body/request"))
                .send()
                .await?;
            if r.status() == 200 || std::time::Instant::now() >= deadline {
                break r.json().await?;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    assert_eq!(req_body["has_body"], Value::Bool(true));
    let request_body_text = req_body["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("request body text must be present for streaming"))?;
    assert!(
        request_body_text.contains("Hi streaming"),
        "streaming request body text missing marker: {request_body_text}"
    );
    Ok(())
}

async fn assert_assembled_response_body(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<()> {
    let resp_body: Value = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let r = client
                .get(format!("{inspect_url}/api/requests/{id}/body/response"))
                .send()
                .await?;
            if r.status() == 200 {
                let body: Value = r.json().await?;
                if body["has_body"] == Value::Bool(true) || std::time::Instant::now() >= deadline {
                    break body;
                }
            } else if std::time::Instant::now() >= deadline {
                anyhow::bail!("/body/response never returned 200 within 10s (status last seen)");
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    assert_eq!(
        resp_body["has_body"],
        Value::Bool(true),
        "assembled streamed response must report has_body=true: {resp_body}"
    );
    let resp_body_byte_size = resp_body["byte_size"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("byte_size missing on assembled response body"))?;
    assert!(
        resp_body_byte_size > 0,
        "assembled streamed response byte_size must be > 0: {resp_body}"
    );
    let resp_body_text = resp_body["text"].as_str().ok_or_else(|| {
        anyhow::anyhow!("assembled response body text must be present: {resp_body}")
    })?;
    let parsed: Value = serde_json::from_str(resp_body_text)?;
    assert_eq!(
        parsed["type"],
        Value::String("message".to_string()),
        "assembled body must be a canonical message object: {parsed}"
    );
    assert_eq!(
        parsed["content"][0]["text"],
        Value::String("hi".to_string()),
        "assembled body's first content block must carry the streamed text: {parsed}"
    );
    Ok(())
}

async fn assert_detail_response_size_reflects_assembled_body(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<()> {
    let detail_resp_size = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let d: Value = client
                .get(format!("{inspect_url}/api/requests/{id}"))
                .send()
                .await?
                .json()
                .await?;
            let size = d["response_body_size"].as_u64().unwrap_or(0);
            if size > 0 || std::time::Instant::now() >= deadline {
                break size;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };
    assert!(
        detail_resp_size > 0,
        "detail response_body_size must reflect the assembled body once persisted, got {detail_resp_size}"
    );
    Ok(())
}

fn assert_ratelimit_headers_present(detail: &Value) -> anyhow::Result<()> {
    let headers = detail["response_headers"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("response_headers must be an array, got: {detail}"))?;
    let has_limit = headers.iter().any(|pair| {
        pair.as_array()
            .and_then(|p| p.first())
            .and_then(|k| k.as_str())
            == Some("anthropic-ratelimit-requests-limit")
    });
    assert!(
        has_limit,
        "streaming detail must include upstream ratelimit headers"
    );
    Ok(())
}

#[tokio::test]
async fn test_streaming_request_recorded_in_store() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let sse_body = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_sse\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":1}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
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

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        )
        .json(&RequestBody::default().stream(true).build())
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let _body = resp.bytes().await?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let requests = loop {
        let list_resp = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?;
        let list_body: Value = list_resp.json().await?;
        let arr = list_body
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("expected array"))?
            .clone();
        let resolved = arr
            .first()
            .and_then(|r| r["status"].as_str())
            .is_some_and(|s| s != "Pending");
        if resolved || std::time::Instant::now() >= deadline {
            break arr;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    assert_eq!(requests.len(), 1, "should have 1 recorded request");
    assert_eq!(requests[0]["is_streaming"], true);
    assert_eq!(requests[0]["model"], "claude-sonnet-4-20250514");
    let status = requests[0]["status"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("expected status string"))?;
    assert!(
        status == "Streaming" || status == "Complete",
        "streaming request status should be Streaming or Complete, got: {status}"
    );

    test_db.cleanup().await?;
    Ok(())
}
