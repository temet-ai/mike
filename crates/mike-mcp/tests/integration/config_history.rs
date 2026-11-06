use tokio::io::AsyncWriteExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    call_tool_request, read_next_message, spawn_mcp_with_inspect, structured_content,
};

#[tokio::test]
async fn list_config_changes_equals_the_rest_body_field_for_field() -> anyhow::Result<()> {
    let prompt = "x".repeat(20_000);
    let rest_body = serde_json::json!({
        "changes": [{
            "id": 12,
            "changed_at": "2026-09-24T10:00:00Z",
            "xact_id": "991",
            "subject_kind": "steerer",
            "subject_key": "4",
            "op": "soft_delete",
            "old_value": {"id": 4, "prompt": prompt, "deleted_at": null},
            "new_value": {"id": 4, "prompt": prompt, "deleted_at": "2026-09-24T10:00:00Z"},
            "redacted": false,
            "via": "spa",
            "actor": "Mozilla/5.0",
            "request_id": "req-1",
            "build_sha": "abc",
            "db_user": "devuser",
            "application_name": "claude-proxy",
            "detail": {}
        }],
        "next_before_id": null
    });
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/config/change-log"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&rest_body))
        .mount(&server)
        .await;
    let (mut child, mut stdin, mut reader) = spawn_mcp_with_inspect(&server.uri()).await?;
    stdin
        .write_all(
            format!(
                "{}\n",
                call_tool_request(21, "list_config_changes", serde_json::json!({}))
            )
            .as_bytes(),
        )
        .await?;
    let response = read_next_message(&mut reader).await?;
    let structured = structured_content(&response)?;
    let body = structured
        .get("change_log")
        .ok_or_else(|| anyhow::anyhow!("envelope must carry `change_log`: {response}"))?;
    assert_eq!(body, &rest_body);
    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
