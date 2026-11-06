use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn ensure_parents(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         ON CONFLICT (project_path)
         DO UPDATE SET last_seen = GREATEST(projects.projects.last_seen, EXCLUDED.last_seen)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(project_id)
}

async fn assert_genuine_send_receipt(
    client: &reqwest::Client,
    inspect_url: &str,
    sender: &str,
    recipient: &str,
) -> anyhow::Result<String> {
    let send_resp = client
        .post(format!("{inspect_url}/api/sessions/{recipient}/message"))
        .json(&json!({
            "message": "did you get this?",
            "from_session_id": sender,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        send_resp.status() == 201,
        "send returned {}",
        send_resp.status()
    );
    let sent: Value = send_resp.json().await?;
    let message_uuid = sent["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid on send response: {sent}"))?
        .to_string();

    let receipt_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{sender}/messages/{message_uuid}/receipt"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        receipt_resp.status() == 200,
        "genuine send must return a receipt 200, got {}",
        receipt_resp.status()
    );
    let receipt: Value = receipt_resp.json().await?;
    anyhow::ensure!(
        receipt["message_uuid"].as_str() == Some(message_uuid.as_str()),
        "receipt must echo the message_uuid: {receipt}"
    );
    anyhow::ensure!(
        receipt["to_session_uuid"].as_str() == Some(recipient),
        "receipt must carry the true recipient: {receipt}"
    );
    anyhow::ensure!(
        receipt["from_session_uuid"].as_str() == Some(sender),
        "receipt must carry the true sender: {receipt}"
    );
    anyhow::ensure!(
        receipt["body"].as_str() == Some("did you get this?"),
        "receipt must carry the real body verbatim: {receipt}"
    );
    anyhow::ensure!(
        receipt["status"].as_str() == Some("pending"),
        "a freshly queued, undelivered message is status pending: {receipt}"
    );
    anyhow::ensure!(
        receipt["sent_request_id"].is_null()
            && receipt["sent_at"].is_null()
            && receipt["consumed_at"].is_null()
            && receipt["dead_lettered_at"].is_null(),
        "a freshly queued message has no delivery/consumption timestamps yet: {receipt}"
    );
    anyhow::ensure!(
        receipt["delivery_attempts"].as_i64() == Some(0),
        "a freshly queued message has zero delivery attempts: {receipt}"
    );
    Ok(message_uuid)
}

#[tokio::test]
async fn genuine_send_returns_receipt_forged_and_wrong_sender_404() -> anyhow::Result<()> {
    use wiremock::MockServer;

    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_receipt");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let sender = "cc000000-0000-4000-c000-000000197001";
    let recipient = "cc000000-0000-4000-c000-000000197002";
    let bystander = "cc000000-0000-4000-c000-000000197003";
    ensure_parents(&pool, sender, "mailbox-197-receipt-test").await?;
    ensure_parents(&pool, recipient, "mailbox-197-receipt-test").await?;
    ensure_parents(&pool, bystander, "mailbox-197-receipt-test").await?;

    let client = reqwest::Client::new();

    let message_uuid =
        assert_genuine_send_receipt(&client, &inspect_url, sender, recipient).await?;

    let forged_uuid = "deadbeef-0000-4000-8000-000000000000";
    let forged_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{sender}/messages/{forged_uuid}/receipt"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        forged_resp.status() == 404,
        "forged message_uuid must NOT produce a receipt — got {}",
        forged_resp.status()
    );

    let laundered_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{bystander}/messages/{message_uuid}/receipt"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        laundered_resp.status() == 404,
        "a real row sent by a DIFFERENT session must not produce a receipt for the \
         bystander — got {}",
        laundered_resp.status()
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn dead_lettered_send_status_visible_via_receipt() -> anyhow::Result<()> {
    use wiremock::MockServer;

    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_receipt");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let sender = "cc000000-0000-4000-c000-000000181001";
    let recipient = "cc000000-0000-4000-c000-000000181002";
    ensure_parents(&pool, sender, "mailbox-181-dead-letter-test").await?;
    ensure_parents(&pool, recipient, "mailbox-181-dead-letter-test").await?;

    let client = reqwest::Client::new();

    let send_resp = client
        .post(format!("{inspect_url}/api/sessions/{recipient}/message"))
        .json(&json!({
            "message": "a blocking instruction that never lands",
            "from_session_id": sender,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        send_resp.status() == 201,
        "send returned {}",
        send_resp.status()
    );
    let sent: Value = send_resp.json().await?;
    let message_uuid = sent["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing message_uuid on send response: {sent}"))?
        .to_string();

    sqlx::query(
        "UPDATE mailbox.messages
            SET claimed_at        = now(),
                sent_at           = now(),
                delivery_attempts = 5,
                dead_lettered_at  = now()
          WHERE message_uuid = $1::text::uuid",
    )
    .bind(&message_uuid)
    .execute(&pool)
    .await?;

    let receipt_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{sender}/messages/{message_uuid}/receipt"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        receipt_resp.status() == 200,
        "a dead-lettered row is still a real row — receipt must still 200, got {}",
        receipt_resp.status()
    );
    let receipt: Value = receipt_resp.json().await?;
    anyhow::ensure!(
        receipt["status"].as_str() == Some("dead_lettered"),
        "dead-lettering must be the honest derived status, not silent success: {receipt}"
    );
    anyhow::ensure!(
        !receipt["dead_lettered_at"].is_null(),
        "dead_lettered_at must be present on the sender's receipt: {receipt}"
    );
    anyhow::ensure!(
        receipt["delivery_attempts"].as_i64() == Some(5),
        "delivery_attempts must reflect the exhausted budget: {receipt}"
    );
    anyhow::ensure!(
        receipt["consumed_at"].is_null(),
        "a dead-lettered row was never consumed: {receipt}"
    );

    let messages_resp = client
        .get(format!("{inspect_url}/api/sessions/{sender}/messages"))
        .send()
        .await?;
    anyhow::ensure!(messages_resp.status() == 200, "{}", messages_resp.status());
    let messages: Value = messages_resp.json().await?;
    let outbox_row = messages["outbox"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["message_uuid"] == message_uuid))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "dead-lettered message missing from the sender's own outbox: {messages}"
            )
        })?;
    anyhow::ensure!(
        outbox_row["status"].as_str() == Some("dead_lettered"),
        "outbox listing must also carry the dead-lettered status: {outbox_row}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
