use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::MockServer;

use super::support::ensure_session;

#[tokio::test]
async fn context_url_kind_rejects_non_http_schemes_on_both_routes() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_message_context (url-scheme guard)");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let alice = "dd000000-0000-4000-d000-000000291001";
    let bob = "dd000000-0000-4000-d000-000000291002";
    ensure_session(&pool, alice, "mailbox-context-url-test").await?;
    ensure_session(&pool, bob, "mailbox-context-url-test").await?;

    let client = reqwest::Client::new();

    let bad_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "should never land",
            "context": [{"kind": "url", "value": "javascript:alert(1)"}],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        bad_resp.status() == 400,
        "javascript: url must 400 on the session route, got {}",
        bad_resp.status()
    );
    let bad_body: Value = bad_resp.json().await?;
    anyhow::ensure!(
        bad_body["error"].as_str() == Some("invalid_context_url"),
        "error code must be invalid_context_url: {bad_body}"
    );
    anyhow::ensure!(
        bad_body["message"]
            .as_str()
            .is_some_and(|m| m.contains("javascript:alert(1)")),
        "error message must name the offending value: {bad_body}"
    );

    let ok_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "safe link",
            "context": [{"kind": "url", "value": "https://example.com/runbook"}],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        ok_resp.status() == 201,
        "https:// url must still be accepted on the session route, got {}",
        ok_resp.status()
    );

    let thread_create_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "thread root",
            "from_session_id": alice,
            "subject": "url-scheme guard thread",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        thread_create_resp.status() == 201,
        "thread create returned {}",
        thread_create_resp.status()
    );
    let threads_resp = client
        .get(format!("{inspect_url}/api/sessions/{alice}/threads"))
        .send()
        .await?;
    let threads: Value = threads_resp.json().await?;
    let thread_uuid = threads["threads"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("threads not an array: {threads}"))?
        .iter()
        .find(|t| t["subject"].as_str() == Some("url-scheme guard thread"))
        .and_then(|t| t["thread_uuid"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {threads}"))?
        .to_string();

    let thread_bad_resp = client
        .post(format!("{inspect_url}/api/threads/{thread_uuid}/message"))
        .json(&json!({
            "body": "should never land",
            "context": [{"kind": "url", "value": "javascript:alert(1)"}],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        thread_bad_resp.status() == 400,
        "javascript: url must 400 on the thread route, got {}",
        thread_bad_resp.status()
    );
    let thread_bad_body: Value = thread_bad_resp.json().await?;
    anyhow::ensure!(
        thread_bad_body["error"].as_str() == Some("invalid_context_url"),
        "error code must be invalid_context_url on the thread route: {thread_bad_body}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
