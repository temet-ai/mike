use crate::anthropic_fixtures;

use anthropic_fixtures::MockResponse;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{boot_small_cap_proxy, send_request};

#[tokio::test]
async fn get_session_falls_back_to_db_after_eviction() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_db_fallback_sess")
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

    send_request(&client, &capped.proxy_url, &session_one, "sess-fallback").await?;
    send_request(&client, &capped.proxy_url, &session_two, "other-sess").await?;

    let sess_resp = client
        .get(format!("{}/api/sessions/{session_one}", capped.inspect_url))
        .send()
        .await?;
    assert_eq!(
        sess_resp.status(),
        200,
        "evicted session must still be reachable via DB fallback"
    );
    let detail: Value = sess_resp.json().await?;
    let summary = detail
        .get("summary")
        .ok_or_else(|| anyhow::anyhow!("missing summary"))?;
    assert_eq!(
        summary.get("id").and_then(|v| v.as_str()),
        Some(session_one.as_str()),
        "session summary id must match"
    );
    let requests = detail
        .get("requests")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert!(
        !requests.is_empty(),
        "DB-fallback session detail must list the persisted request(s)"
    );

    let missing = client
        .get(format!(
            "{}/api/sessions/totally-not-a-real-session-{}",
            capped.inspect_url,
            uuid::Uuid::new_v4()
        ))
        .send()
        .await?;
    assert_eq!(
        missing.status(),
        404,
        "unknown session id must still 404 after DB lookup"
    );

    capped.shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    capped.test_db.cleanup().await?;
    Ok(())
}

fn assert_required_summary_fields(row: &Value) -> anyhow::Result<()> {
    let required_keys = [
        "id",
        "session_id",
        "method",
        "path",
        "model",
        "status",
        "is_streaming",
        "message_count",
        "max_tokens",
        "created_at",
        "completed_at",
        "project",
        "human_prompt",
        "turn_type",
        "turn_origin",
        "tool_names",
        "response_tool_count",
        "response_tools_used",
    ];
    for key in required_keys {
        assert!(
            row.get(key).is_some(),
            "RequestSummary field `{key}` missing from DB-fallback listing row: {row}"
        );
    }
    assert!(
        row.get("subagent_persona").is_none(),
        "subagent_persona must be omitted on a non-subagent request (skip_serializing_if = None)"
    );
    assert!(
        row.get("response_preview").is_none(),
        "response_preview must be absent on the DB-fallback listing row — dropped in Phase D"
    );
    Ok(())
}

fn assert_summary_field_values(row: &Value, session_one: &str) -> anyhow::Result<()> {
    assert_eq!(
        row.get("session_id").and_then(|v| v.as_str()),
        Some(session_one),
        "DB-fallback row session_id must match the captured session"
    );
    assert_eq!(
        row.get("method").and_then(|v| v.as_str()),
        Some("POST"),
        "DB-fallback row method must be POST"
    );
    assert_eq!(
        row.get("path").and_then(|v| v.as_str()),
        Some("/v1/messages"),
        "DB-fallback row path must be /v1/messages"
    );
    assert_eq!(
        row.get("status").and_then(|v| v.as_str()),
        Some("Complete"),
        "DB-fallback row status must round-trip the terminal status (Title-Case via normalize_status_code)"
    );
    assert_eq!(
        row.get("max_tokens").and_then(|v| v.as_u64()),
        Some(4),
        "DB-fallback row max_tokens must be extracted from params JSONB"
    );
    assert!(
        row.get("project").map(Value::is_null).unwrap_or(false),
        "DB-fallback row project must be JSON null"
    );
    assert!(
        row.get("response_tools_used")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "non-tool request must have an empty response_tools_used array, not be missing"
    );
    assert_eq!(
        row.get("response_tool_count").and_then(|v| v.as_u64()),
        Some(0),
        "non-tool request must have response_tool_count == 0"
    );
    Ok(())
}

#[tokio::test]
async fn get_session_db_fallback_listing_populates_request_summary_fields() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(MockResponse::new("msg_summary_shape").usage(1, 1).build())
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

    send_request(
        &client,
        &capped.proxy_url,
        &session_one,
        "summary-shape-test",
    )
    .await?;
    send_request(&client, &capped.proxy_url, &session_two, "evict-trigger").await?;

    let detail: Value = client
        .get(format!("{}/api/sessions/{session_one}", capped.inspect_url))
        .send()
        .await?
        .json()
        .await?;

    let requests = detail
        .get("requests")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    let row = requests
        .first()
        .ok_or_else(|| anyhow::anyhow!("DB-fallback summary must list at least one request"))?;

    assert_required_summary_fields(row)?;
    assert_summary_field_values(row, &session_one)?;

    capped.shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    capped.test_db.cleanup().await?;
    Ok(())
}
