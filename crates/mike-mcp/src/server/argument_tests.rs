use super::*;
use serde_json::{Map, Value, json};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

fn server(mock: &MockServer) -> McpServer {
    McpServer::new(InspectClient::new(mock.uri()), Profile::Full, None)
}

#[tokio::test]
async fn dispatch_tool_find_text_query_is_sent_as_needle_and_reported() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"matches": []})))
        .mount(&mock)
        .await;
    let response = server(&mock)
        .dispatch_tool("find_text", args(json!({"query": "abc"})))
        .await?;
    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("mock did not record requests"))?;
    let sent = requests
        .first()
        .ok_or_else(|| anyhow::anyhow!("no request reached the inspect API"))?;
    assert!(sent.url.query().is_some_and(|q| q.contains("needle=abc")));
    let rmcp::model::CallToolResponse::Complete(result) = response else {
        anyhow::bail!("expected a completed tool call");
    };
    let structured = result
        .structured_content
        .ok_or_else(|| anyhow::anyhow!("expected structured content"))?;
    assert_eq!(
        structured["arguments_corrected"],
        json!([{"sent": "query", "used": "needle"}])
    );
    Ok(())
}

#[tokio::test]
async fn dispatch_tool_find_text_whole_bodies_and_include_reminders_are_declared_and_passed_through()
-> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "matches": [], "scan_complete": true, "full_match_count": 0
        })))
        .mount(&mock)
        .await;

    server(&mock)
        .dispatch_tool(
            "find_text",
            args(json!({
                "needle": "abc",
                "session_id": "11111111-1111-4111-8111-111111111111",
                "whole_bodies": true,
                "include_reminders": true
            })),
        )
        .await?;

    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("mock did not record requests"))?;
    let query = requests
        .first()
        .and_then(|sent| sent.url.query())
        .ok_or_else(|| anyhow::anyhow!("no request reached the inspect API"))?;
    assert!(query.contains("whole_bodies=true"), "{query}");
    assert!(query.contains("include_reminders=true"), "{query}");
    Ok(())
}

#[tokio::test]
async fn dispatch_tool_unknown_argument_is_refused_without_an_http_call() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let error = server(&mock)
        .dispatch_tool("read_project_knowledge", args(json!({"ids": ["a"]})))
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("an unknown argument was accepted"))?;
    assert!(error.message.contains("unknown argument `ids`"));
    assert!(
        error
            .message
            .contains("reference (required), project, select")
    );
    let requests = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("mock did not record requests"))?;
    assert!(requests.is_empty());
    Ok(())
}
