use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::common::{
    call_tool_request, read_next_message, spawn_mcp_with_inspect, structured_content,
};

#[tokio::test]
async fn get_request_summary_tool_returns_summary_envelope() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/requests/99/summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 99,
            "method": "POST",
            "path": "/v1/messages",
            "status": "Complete",
            "human_prompt": "hi",
            "tool_names": []
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(10, "get_request_summary", serde_json::json!({"id": 99}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let summary = structured
        .get("summary")
        .ok_or_else(|| anyhow::anyhow!("envelope must carry `summary`: {response}"))?;
    assert_eq!(summary["id"], 99);
    assert_eq!(summary["status"], "Complete");

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn get_request_body_tool_returns_body_envelope() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/requests/42/body/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "{\"model\":\"claude-opus-4-7\"}",
            "byte_size": 27,
            "has_body": true
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(11, "get_request_body", serde_json::json!({"id": 42}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let body = structured
        .get("body")
        .ok_or_else(|| anyhow::anyhow!("envelope must carry `body`: {response}"))?;
    assert_eq!(body["byte_size"], 27);
    assert_eq!(body["has_body"], true);
    assert!(
        body["text"]
            .as_str()
            .unwrap_or_default()
            .contains("claude-opus-4-7")
    );

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn get_request_sent_body_tool_hits_request_sent_path() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/requests/42/body/request_sent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "filtered",
            "byte_size": 8,
            "has_body": true
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(12, "get_request_sent_body", serde_json::json!({"id": 42}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let body = structured
        .get("body")
        .ok_or_else(|| anyhow::anyhow!("envelope must carry `body`"))?;
    assert_eq!(body["text"], "filtered");

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn get_response_body_tool_hits_response_path() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/requests/42/body/response"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "event: message_start\ndata: {...}\n\n",
            "byte_size": 33,
            "has_body": true
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(13, "get_response_body", serde_json::json!({"id": 42}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let body = structured
        .get("body")
        .ok_or_else(|| anyhow::anyhow!("envelope must carry `body`"))?;
    assert!(
        body["text"]
            .as_str()
            .unwrap_or_default()
            .contains("message_start")
    );

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn diagnostic_findings_tool_returns_findings_envelope() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/diagnostics/findings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "request_id": 7,
                "session_id": "sess-1",
                "category": "Clarity",
                "analyzer": "weasel.placeholder",
                "advice": "avoid",
                "matched_text": "basically",
                "match_range": [0, 9],
                "other_hits": 0,
                "created_at": "2026-04-23T19:00:00Z",
                "request": {"id": 7, "path": "/v1/messages"}
            }
        ])))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(14, "diagnostic_findings", serde_json::json!({}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let findings = structured
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("envelope must carry `findings` array: {response}"))?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["analyzer"], "weasel.placeholder");

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn get_stats_tool_returns_runtime_diagnostic_and_health_shape() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "request_count": 12,
            "active_streams": 1,
            "uptime_seconds": 99
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/diagnostics/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "totals_by_category": {"Clarity": 1},
            "totals_by_analyzer": {"weasel.placeholder": 1},
            "last_24h": 1
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok"
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(5, "get_stats", serde_json::json!({}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let stats = structured
        .get("stats")
        .ok_or_else(|| anyhow::anyhow!("envelope missing `stats` key"))?;

    let runtime = stats
        .get("runtime")
        .ok_or_else(|| anyhow::anyhow!("stats.runtime missing"))?;
    let diagnostic = stats
        .get("diagnostic")
        .ok_or_else(|| anyhow::anyhow!("stats.diagnostic missing"))?;
    let health = stats
        .get("health")
        .ok_or_else(|| anyhow::anyhow!("stats.health missing"))?;

    assert_eq!(runtime["request_count"], 12);
    assert_eq!(diagnostic["last_24h"], 1);
    assert_eq!(health["status"], "ok");

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
