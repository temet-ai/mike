use serde_json::json;

use crate::change_log::Stack;

#[tokio::test]
async fn patching_rerank_settings_reaches_the_running_service_without_a_restart()
-> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let service = stack.state.rerank_service();
    assert!(
        !service.is_enabled(),
        "a fresh install starts with the reranker off"
    );

    stack
        .patch("/api/settings/rerank.enabled", json!({"value": true}), None)
        .await?;
    stack
        .patch(
            "/api/settings/rerank.url",
            json!({"value": "http://127.0.0.1:1/"}),
            None,
        )
        .await?;
    stack
        .patch(
            "/api/settings/rerank.timeout_ms",
            json!({"value": 250}),
            None,
        )
        .await?;
    stack
        .patch(
            "/api/settings/rerank.candidates",
            json!({"value": 20}),
            None,
        )
        .await?;

    let config = service.current_config();
    assert!(config.enabled);
    assert_eq!(config.url, "http://127.0.0.1:1");
    assert_eq!(config.timeout_ms, 250);
    assert_eq!(config.candidates, 20);
    let health = stack.get("/api/health").await?;
    assert_eq!(health["rerank"]["enabled"], json!(true));
    stack.finish().await
}

#[tokio::test]
async fn an_unreachable_reranker_shows_degraded_in_health_and_capabilities() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    stack
        .patch("/api/settings/rerank.enabled", json!({"value": true}), None)
        .await?;
    stack
        .patch(
            "/api/settings/rerank.url",
            json!({"value": "http://127.0.0.1:1"}),
            None,
        )
        .await?;
    stack.state.rerank_service().health().await;

    let health = stack.get("/api/health").await?;
    assert_eq!(health["rerank"]["state"], json!("degraded"));
    assert!(health["rerank"]["fix"].is_string(), "{health}");
    let capabilities = stack.get("/api/capabilities").await?;
    let rows = capabilities["capabilities"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("capabilities must be an array: {capabilities}"))?;
    let row = rows
        .iter()
        .find(|row| row["id"] == json!("rerank"))
        .ok_or_else(|| anyhow::anyhow!("no rerank row in {capabilities}"))?;
    assert_eq!(row["state"], json!("degraded"));
    assert_ne!(row["fix"], json!(""));
    stack.finish().await
}

#[tokio::test]
async fn patching_rerank_timeout_below_the_minimum_is_rejected_and_changes_nothing()
-> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let response = reqwest::Client::new()
        .patch(format!("{}/api/settings/rerank.timeout_ms", stack.base))
        .json(&json!({"value": 10}))
        .send()
        .await?;

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        stack.state.rerank_service().current_config().timeout_ms,
        800
    );
    stack.finish().await
}
