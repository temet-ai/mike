use tokio::io::AsyncWriteExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    call_tool_request, read_next_message, spawn_mcp_with_inspect, structured_content,
};

#[tokio::test]
async fn search_session_content_forwards_order_to_the_content_search_route() -> anyhow::Result<()> {
    let rest_body = serde_json::json!({
        "query": "workflow",
        "order": "recent",
        "total_matches": 0,
        "total_sessions": 0,
        "sessions": [],
        "has_more": false
    });
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/content/search"))
        .and(query_param("q", "workflow"))
        .and(query_param("order", "recent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&rest_body))
        .expect(1)
        .mount(&server)
        .await;
    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(
                    31,
                    "search_session_content",
                    serde_json::json!({"q": "workflow", "order": "recent"})
                )
            )
            .as_bytes(),
        )
        .await?;

    let response = read_next_message(&mut reader).await?;

    let structured = structured_content(&response)?;
    assert_eq!(structured.get("content_search"), Some(&rest_body));
    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
