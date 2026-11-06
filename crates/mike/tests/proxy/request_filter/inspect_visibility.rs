use crate::filter_proxy;

use crate::anthropic_fixtures;

use super::support::start_proxy_dropping_system_reminder;
use anthropic_fixtures::{RequestBody, text_block};
use filter_proxy::{claude_response_json, session_id};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn filter_result_visible_via_inspect_api() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };
    let proxy_url = &proxy.proxy_url;
    let inspect_url = &proxy.inspect_url;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([
                    text_block("Preamble."),
                    text_block("<system-reminder>SECRET</system-reminder>"),
                ]))
                .content("hi")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(list_resp.status(), 200);
    let requests: Value = list_resp.json().await?;

    let requests = requests
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be array"))?;
    assert_eq!(requests.len(), 1, "should have exactly 1 stored request");

    let request_id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be a number"))?;

    assert_detail_omits_retired_filter_fields(&client, inspect_url, request_id).await?;
    assert_sent_body_reflects_drop(&client, inspect_url, request_id).await?;
    assert_original_body_preserves_dropped_content(&client, inspect_url, request_id).await?;

    proxy.cleanup().await?;
    Ok(())
}

async fn assert_detail_omits_retired_filter_fields(
    client: &reqwest::Client,
    inspect_url: &str,
    request_id: u64,
) -> anyhow::Result<()> {
    let detail_resp = client
        .get(format!("{inspect_url}/api/requests/{request_id}"))
        .send()
        .await?;
    assert_eq!(detail_resp.status(), 200);
    let detail: Value = detail_resp.json().await?;
    assert!(
        detail.get("filter_applied").is_none(),
        "filter_applied must NOT be on the wire — retired from RequestDetail"
    );
    assert!(
        detail.get("filtered_request_body_size").is_none(),
        "filtered_request_body_size must NOT be on the wire — retired"
    );
    Ok(())
}

async fn assert_sent_body_reflects_drop(
    client: &reqwest::Client,
    inspect_url: &str,
    request_id: u64,
) -> anyhow::Result<()> {
    let sent_resp = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/body/request_sent"
        ))
        .send()
        .await?;
    assert_eq!(sent_resp.status(), 200);
    let sent_payload: Value = sent_resp.json().await?;
    assert!(
        sent_payload["has_body"].as_bool().unwrap_or(false),
        "sent body must have content"
    );
    let sent_text = sent_payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("sent body text must be a string"))?;
    let sent_body: Value = serde_json::from_str(sent_text)?;
    let sent_arr = sent_body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("sent body system must be array"))?;
    assert_eq!(sent_arr.len(), 1, "reminder dropped in sent body");
    assert_eq!(sent_arr[0]["text"], "Preamble.");
    assert!(
        !sent_text.contains("SECRET"),
        "sent body must not contain filtered content"
    );
    Ok(())
}

async fn assert_original_body_preserves_dropped_content(
    client: &reqwest::Client,
    inspect_url: &str,
    request_id: u64,
) -> anyhow::Result<()> {
    let orig_resp = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/body/request"
        ))
        .send()
        .await?;
    assert_eq!(orig_resp.status(), 200);
    let orig_payload: Value = orig_resp.json().await?;
    let orig_text = orig_payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("original body text must be a string"))?;
    let orig_body: Value = serde_json::from_str(orig_text)?;
    let orig_arr = orig_body["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("original body system must be array"))?;
    assert_eq!(
        orig_arr.len(),
        2,
        "original body preserves both blocks (pre-filter)"
    );
    assert!(
        orig_text.contains("SECRET"),
        "original body must preserve filtered content"
    );
    Ok(())
}

#[tokio::test]
async fn sent_body_is_byte_identical_to_upstream_and_manifest_records_filter_stage()
-> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(claude_response_json())
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_dropping_system_reminder(&mock_server.uri()).await? else {
        return Ok(());
    };
    let proxy_url = &proxy.proxy_url;
    let inspect_url = &proxy.inspect_url;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([
                    text_block("Preamble."),
                    text_block("<system-reminder>SECRET</system-reminder>"),
                ]))
                .content("hi")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);
    let upstream_bytes = received[0].body.clone();

    let list_resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(list_resp.status(), 200);
    let requests: Value = list_resp.json().await?;
    let requests = requests
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be array"))?;
    assert_eq!(requests.len(), 1, "should have exactly 1 stored request");
    let request_id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be a number"))?;

    let sent_resp = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/body/request_sent"
        ))
        .send()
        .await?;
    assert_eq!(sent_resp.status(), 200);
    let sent_payload: Value = sent_resp.json().await?;
    assert!(sent_payload["has_body"].as_bool().unwrap_or(false));
    let sent_text = sent_payload["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("sent body text must be a string"))?;
    assert_eq!(
        sent_text.as_bytes(),
        upstream_bytes.as_slice(),
        "get_request_sent_body must return the exact bytes forwarded upstream"
    );

    let manifest_resp = client
        .get(format!("{inspect_url}/api/requests/{request_id}/manifest"))
        .send()
        .await?;
    assert_eq!(manifest_resp.status(), 200);
    let manifest_payload: Value = manifest_resp.json().await?;
    assert_eq!(manifest_payload["has_manifest"], true);
    let manifest = &manifest_payload["manifest"];
    assert_eq!(manifest["schema_version"], 1);
    let stages = manifest["stages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("stages must be an array"))?;
    assert!(!stages.is_empty(), "filter stage must be recorded");
    assert!(
        stages
            .iter()
            .any(|s| s["stage"] == "filter" && s["outcome"] == "applied"),
        "manifest must record the filter stage as applied: {stages:?}"
    );

    proxy.cleanup().await?;
    Ok(())
}
