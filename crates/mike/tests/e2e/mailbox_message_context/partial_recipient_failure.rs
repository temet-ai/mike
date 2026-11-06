use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::MockServer;

use super::support::ensure_session;

#[tokio::test]
async fn thread_message_partial_recipient_failure_delivers_the_rest_and_names_the_failure()
-> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_message_context (partial failure)");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let alice = "dd000000-0000-4000-d000-000000293001";
    let bob = "dd000000-0000-4000-d000-000000293002";
    let nonexistent = "dd000000-0000-4000-d000-000000293999";
    ensure_session(&pool, alice, "mailbox-thread-partial-fail-test").await?;
    ensure_session(&pool, bob, "mailbox-thread-partial-fail-test").await?;

    let client = reqwest::Client::new();

    let thread_create_resp = client
        .post(format!("{inspect_url}/api/sessions/{bob}/message"))
        .json(&json!({
            "message": "thread root",
            "from_session_id": alice,
            "subject": "partial-failure thread",
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
        .find(|t| t["subject"].as_str() == Some("partial-failure thread"))
        .and_then(|t| t["thread_uuid"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {threads}"))?
        .to_string();

    let send_resp = client
        .post(format!("{inspect_url}/api/threads/{thread_uuid}/message"))
        .json(&json!({
            "body": "one good, one bad",
            "from_session_id": alice,
            "to": [bob, nonexistent],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        send_resp.status() == 201,
        "a partial recipient failure must still 201 the successful deliveries, got {}: {}",
        send_resp.status(),
        send_resp.text().await.unwrap_or_default()
    );
    let sent: Value = send_resp.json().await?;
    let deliveries = sent["deliveries"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("deliveries not an array: {sent}"))?;
    anyhow::ensure!(
        deliveries.len() == 1,
        "exactly one recipient must succeed: {sent}"
    );
    anyhow::ensure!(
        deliveries[0]["to_session_uuid"].as_str() == Some(bob),
        "the surviving delivery must address bob: {sent}"
    );
    let failed = sent["failed"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("failed not an array: {sent}"))?;
    anyhow::ensure!(
        failed.len() == 1 && failed[0]["to_session_uuid"].as_str() == Some(nonexistent),
        "the nonexistent participant must be named in failed: {sent}"
    );
    anyhow::ensure!(
        failed[0]["reason"].as_str() == Some("session_not_found"),
        "the failure reason must name why the recipient failed: {sent}"
    );

    let message_uuid = deliveries[0]["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {sent}"))?;
    let (row_exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM mailbox.messages WHERE message_uuid = $1::text::uuid)",
    )
    .bind(message_uuid)
    .fetch_one(&pool)
    .await?;
    anyhow::ensure!(
        row_exists,
        "the successful delivery's mailbox.messages row must persist despite the sibling failure"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
