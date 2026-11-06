use crate::db_helpers;
use serde_json::{Value, json};

use super::support::{
    follow_up_body_with_tool_result, insert_emitting_request, start_upstream_stub,
};

#[tokio::test]
async fn capture_attaches_string_content_tool_result_to_emitting_request() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440001";
    let project_path = "test-project-string-content";
    let tool_use_id = "toolu_01abcdef";

    let emitting_id =
        insert_emitting_request(&pool, session_id, project_path, tool_use_id, "Bash", 0).await?;

    let follow_up = follow_up_body_with_tool_result(
        session_id,
        tool_use_id,
        false,
        json!("first 10 lines:\n1\n2\n3"),
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(&follow_up)
        .send()
        .await?;
    assert!(
        resp.status().is_success(),
        "proxy roundtrip failed: {}",
        resp.status()
    );

    let resp = client
        .get(format!(
            "{inspect_url}/api/requests/{emitting_id}/tool-results"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["request_id"].as_u64(), Some(emitting_id as u64));
    assert_eq!(body["total_count"].as_u64(), Some(1));
    let results = body["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("results must be an array"))?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["tool_use_id"].as_str(), Some(tool_use_id));
    assert_eq!(results[0]["is_error"].as_bool(), Some(false));
    assert_eq!(
        results[0]["content"],
        Value::String("first 10 lines:\n1\n2\n3".to_string()),
        "string content must round-trip byte-identical"
    );
    assert_eq!(
        results[0]["content_sha256"]
            .as_str()
            .map(str::len)
            .unwrap_or(0),
        64,
        "content_sha256 must be a 64-char hex string"
    );
    assert!(results[0]["content_bytes"].as_u64().unwrap_or(0) > 0);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_preserves_array_content_blocks_verbatim() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440002";
    let project_path = "test-project-array-content";
    let tool_use_id = "toolu_02abcdef";

    let emitting_id =
        insert_emitting_request(&pool, session_id, project_path, tool_use_id, "Bash", 0).await?;

    let array_content = json!([
        {"type": "text", "text": "the first 5 results are"},
        {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KG..."}}
    ]);
    let follow_up =
        follow_up_body_with_tool_result(session_id, tool_use_id, false, array_content.clone());

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(&follow_up)
        .send()
        .await?;
    assert!(resp.status().is_success());

    let resp = client
        .get(format!(
            "{inspect_url}/api/requests/{emitting_id}/tool-results"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    let results = body["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("results must be an array"))?;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["content"], array_content,
        "array content must round-trip byte-identical, including image source data"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
