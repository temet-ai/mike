use crate::db_helpers;

use serde_json::{Value, json};
use wiremock::MockServer;

use super::support::{ensure_session, seed_live_registration, seed_role};

#[tokio::test]
async fn role_fanout_excludes_sender_declaring_the_fanned_out_role() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_tags_addressing (self-exclusion)");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let sender = "dd000000-0000-4000-e005-000000000001";
    let peer = "dd000000-0000-4000-e006-000000000001";
    let project_path = "mailbox-role-self-exclusion-test";

    let (sender_id, project_id) = ensure_session(&pool, sender, project_path).await?;
    let (peer_id, _) = ensure_session(&pool, peer, project_path).await?;

    seed_live_registration(&pool, sender_id, project_id).await?;
    seed_live_registration(&pool, peer_id, project_id).await?;
    seed_role(&pool, sender_id, "on-call").await?;
    seed_role(&pool, peer_id, "on-call").await?;

    let client = reqwest::Client::new();
    let fanout_resp = client
        .post(format!("{inspect_url}/api/sessions/role:on-call/message"))
        .json(&json!({
            "message": "heads up",
            "from_session_id": sender,
            "subject": "self-exclusion check",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        fanout_resp.status() == 201,
        "role: fan-out returned {}: {}",
        fanout_resp.status(),
        fanout_resp.text().await.unwrap_or_default()
    );
    let fanout: Value = fanout_resp.json().await?;
    let deliveries = fanout["deliveries"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("deliveries not an array: {fanout}"))?;
    anyhow::ensure!(
        deliveries.len() == 1,
        "the sender must be excluded even though it declares the fanned-out role: got {} \
         deliveries: {fanout}",
        deliveries.len()
    );
    let recipient = deliveries[0]["to_session_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing to_session_uuid: {fanout}"))?;
    anyhow::ensure!(
        recipient == peer,
        "the only delivery must address the peer, not the sender: got {recipient}"
    );

    let thread_uuid = fanout["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {fanout}"))?;
    let (row_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mailbox.messages m
          JOIN mailbox.threads t ON t.id = m.thread_id
          JOIN sessions.sessions s ON s.id = m.to_session_id
         WHERE t.thread_uuid = $1::text::uuid
           AND s.session_uuid = $2::text::uuid",
    )
    .bind(thread_uuid)
    .bind(sender)
    .fetch_one(&pool)
    .await?;
    anyhow::ensure!(
        row_count == 0,
        "the sender must never get a mailbox.messages row from its own role fan-out: got {row_count}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
