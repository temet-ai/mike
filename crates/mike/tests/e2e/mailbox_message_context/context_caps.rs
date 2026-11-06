use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::MockServer;

use super::support::ensure_session;

#[tokio::test]
async fn context_and_tag_caps_reject_over_limit_sends() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_message_context (caps)");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let alice = "dd000000-0000-4000-d000-000000292001";
    let bob = "dd000000-0000-4000-d000-000000292002";
    ensure_session(&pool, alice, "mailbox-context-caps-test").await?;
    ensure_session(&pool, bob, "mailbox-context-caps-test").await?;

    let client = reqwest::Client::new();

    let too_many_items: Vec<Value> = (0..33)
        .map(|i| json!({"kind": "note", "value": format!("item {i}")}))
        .collect();
    let items_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({ "message": "too many items", "context": too_many_items }))
        .send()
        .await?;
    anyhow::ensure!(
        items_resp.status() == 400,
        "context over the item cap must 400, got {}",
        items_resp.status()
    );
    let items_body: Value = items_resp.json().await?;
    anyhow::ensure!(
        items_body["error"].as_str() == Some("too_many_context_items"),
        "error code must be too_many_context_items: {items_body}"
    );

    let long_value = "x".repeat(2049);
    let value_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "value too long",
            "context": [{"kind": "note", "value": long_value}],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        value_resp.status() == 400,
        "context value over the length cap must 400, got {}",
        value_resp.status()
    );
    let value_body: Value = value_resp.json().await?;
    anyhow::ensure!(
        value_body["error"].as_str() == Some("context_value_too_long"),
        "error code must be context_value_too_long: {value_body}"
    );

    let too_many_tags: Vec<String> = (0..17).map(|i| format!("tag-{i}")).collect();
    let tags_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "too many tags",
            "from_session_id": alice,
            "subject": "caps thread",
            "tags": too_many_tags,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        tags_resp.status() == 400,
        "tags over the count cap must 400, got {}",
        tags_resp.status()
    );
    let tags_body: Value = tags_resp.json().await?;
    anyhow::ensure!(
        tags_body["error"].as_str() == Some("too_many_tags"),
        "error code must be too_many_tags: {tags_body}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
