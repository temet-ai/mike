use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{fire_streaming_request_and_get_id, sse_streaming_body, wait_for_complete};

async fn wait_for_response_body_assembled(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<serde_json::Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let r = client
            .get(format!("{inspect_url}/api/requests/{id}/body/response"))
            .send()
            .await?;
        if r.status() == 200 {
            let b: serde_json::Value = r.json().await?;
            if b["has_body"].as_bool() == Some(true) || std::time::Instant::now() >= deadline {
                return Ok(b);
            }
        } else if std::time::Instant::now() >= deadline {
            anyhow::bail!("/body/response never returned 200 within 10s");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn assert_find_text_matches_split_phrase(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<()> {
    let find: serde_json::Value = client
        .get(format!("{inspect_url}/api/requests/{id}/body/find"))
        .query(&[("needle", "didn't actually"), ("source", "response")])
        .send()
        .await?
        .json()
        .await?;
    let match_count = find["full_match_count"].as_u64().unwrap_or(0);
    assert!(
        match_count >= 1,
        "expected ≥1 response-body match for the split phrase; got: {find}"
    );
    Ok(())
}

fn assert_response_body_contains_contiguous_phrase(body: &serde_json::Value) -> anyhow::Result<()> {
    assert_eq!(
        body["has_body"].as_bool(),
        Some(true),
        "assembled streamed response must report has_body=true: {body}"
    );
    let text = body["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("response body text missing: {body}"))?;
    assert!(
        text.contains("didn't actually"),
        "assembled response body must contain the contiguous phrase; got: {text}"
    );
    let parsed: serde_json::Value = serde_json::from_str(text)?;
    assert_eq!(parsed["type"].as_str(), Some("message"));
    assert_eq!(parsed["role"].as_str(), Some("assistant"));
    assert_eq!(
        parsed["content"][0]["type"].as_str(),
        Some("text"),
        "first content block must be a text block: {parsed}"
    );
    assert_eq!(
        parsed["content"][0]["text"].as_str(),
        Some("didn't actually"),
        "the three text_deltas must coalesce into one contiguous text block"
    );
    Ok(())
}

#[tokio::test]
async fn streamed_split_phrase_is_findable_and_response_body_assembled() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    let chunks = ["didn't", " ", "actually"];
    let sse_body = sse_streaming_body(&chunks);

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let client = reqwest::Client::new();

    let id =
        fire_streaming_request_and_get_id(&client, &proxy.proxy_url, &proxy.inspect_url).await?;

    wait_for_complete(&client, &proxy.inspect_url, id, Duration::from_secs(5)).await?;

    let body = wait_for_response_body_assembled(&client, &proxy.inspect_url, id).await?;

    assert_find_text_matches_split_phrase(&client, &proxy.inspect_url, id).await?;
    assert_response_body_contains_contiguous_phrase(&body)?;

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}
