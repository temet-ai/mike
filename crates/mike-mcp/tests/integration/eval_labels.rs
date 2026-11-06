use tokio::io::AsyncWriteExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    call_tool_request, read_next_message, spawn_mcp_with_inspect, structured_content,
};

#[tokio::test]
async fn list_eval_labels_equals_the_rest_body() -> anyhow::Result<()> {
    let rest = serde_json::json!({
        "labels": [{
            "id": 5, "tier": "seed", "axis": "reaction", "label": "corrected",
            "evidence": "you have now wasted hours on this quick test.".repeat(40),
            "provenance": {"A": {"label": "CORRECTED"}, "B": {"label": "CORRECTED"}, "reason": "agreed"}
        }],
        "next_before_id": null,
        "verdict_now": null
    });
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/eval/labels"))
        .and(query_param("turn_id", "16368"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&rest))
        .mount(&server)
        .await;
    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(
                    41,
                    "list_eval_labels",
                    serde_json::json!({"turn_id": 16368})
                )
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    assert_eq!(structured_content(&response)?.get("labels"), Some(&rest));
    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
