use tokio::io::AsyncWriteExt;

use crate::common::{
    call_tool_request, read_next_message, spawn_mcp_with_inspect, structured_content,
};

#[tokio::test]
async fn get_session_tool_forwards_offset_and_limit_and_surfaces_metadata() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/sessions/sess-xyz"))
        .and(query_param("offset", "5"))
        .and(query_param("limit", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "summary": {"id": "sess-xyz", "request_count": 100},
            "requests": [
                {"id": 6}, {"id": 7}, {"id": 8}
            ],
            "total_requests": 100,
            "offset": 5,
            "limit": 3
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(
                    3,
                    "get_session",
                    serde_json::json!({"id": "sess-xyz", "offset": 5, "limit": 3})
                )
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    assert_eq!(response["id"], 3);

    let structured = structured_content(&response)?;
    let session = structured
        .get("session")
        .ok_or_else(|| anyhow::anyhow!("envelope missing `session` key: {response}"))?;

    assert_eq!(session["total_requests"], 100);
    assert_eq!(session["offset"], 5);
    assert_eq!(session["limit"], 3);
    assert_eq!(session["summary"]["request_count"], 100);
    let requests = session["requests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert_eq!(requests.len(), 3);

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn list_sessions_tool_forwards_every_filter_and_pagination_param() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/sessions"))
        .and(query_param("order", "asc"))
        .and(query_param("date_from", "2026-07-01T00:00:00Z"))
        .and(query_param("date_to", "2026-07-16T00:00:00Z"))
        .and(query_param("min_request_count", "5"))
        .and(query_param("project", "temet-proxy"))
        .and(query_param("fields", "id,request_count"))
        .and(query_param("limit", "3"))
        .and(query_param("offset", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "sess-1", "request_count": 9}
        ])))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(
                    3,
                    "list_sessions",
                    serde_json::json!({
                        "order": "asc",
                        "date_from": "2026-07-01T00:00:00Z",
                        "date_to": "2026-07-16T00:00:00Z",
                        "min_request_count": 5,
                        "project": "temet-proxy",
                        "fields": "id,request_count",
                        "limit": 3,
                        "offset": 10
                    })
                )
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    assert_eq!(response["id"], 3);

    let structured = structured_content(&response)?;
    let sessions = structured
        .get("sessions")
        .ok_or_else(|| anyhow::anyhow!("envelope missing `sessions` key: {response}"))?;
    assert_eq!(sessions[0]["id"], "sess-1");

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn get_session_tool_without_pagination_omits_metadata_fields() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/sessions/sess-legacy"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "summary": {"id": "sess-legacy"},
            "requests": [{"id": 1}]
        })))
        .mount(&server)
        .await;

    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(4, "get_session", serde_json::json!({"id": "sess-legacy"}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let session = structured
        .get("session")
        .ok_or_else(|| anyhow::anyhow!("envelope missing `session` key"))?;
    assert!(session.get("total_requests").is_none());
    assert!(session.get("offset").is_none());
    assert!(session.get("limit").is_none());
    assert_eq!(session["summary"]["id"], "sess-legacy");

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
