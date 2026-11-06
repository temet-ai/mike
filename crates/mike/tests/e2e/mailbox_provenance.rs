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

#[tokio::test]
async fn genuine_delivery_verifies_forged_and_laundered_do_not() -> anyhow::Result<()> {
    use wiremock::MockServer;

    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping mailbox_provenance");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let sender = "cc000000-0000-4000-c000-000000173001";
    let recipient = "cc000000-0000-4000-c000-000000173002";
    let bystander = "cc000000-0000-4000-c000-000000173003";
    ensure_parents(&pool, sender, "mailbox-173-provenance-test").await?;
    ensure_parents(&pool, recipient, "mailbox-173-provenance-test").await?;
    ensure_parents(&pool, bystander, "mailbox-173-provenance-test").await?;

    let client = reqwest::Client::new();

    let send_resp = client
        .post(format!("{inspect_url}/api/sessions/{recipient}/message"))
        .json(&json!({
            "message": "for real: please review PR #1",
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

    let verify_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{recipient}/messages/{message_uuid}"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        verify_resp.status() == 200,
        "genuine delivery must verify 200, got {}",
        verify_resp.status()
    );
    let verified: Value = verify_resp.json().await?;
    anyhow::ensure!(
        verified["message_uuid"].as_str() == Some(message_uuid.as_str()),
        "verified row must echo the message_uuid: {verified}"
    );
    anyhow::ensure!(
        verified["to_session_uuid"].as_str() == Some(recipient),
        "verified row must carry the true recipient: {verified}"
    );
    anyhow::ensure!(
        verified["from_session_uuid"].as_str() == Some(sender),
        "verified row must carry the true sender: {verified}"
    );
    anyhow::ensure!(
        verified["body"].as_str() == Some("for real: please review PR #1"),
        "verified row must carry the real body verbatim: {verified}"
    );

    let forged_uuid = "deadbeef-0000-4000-8000-000000000000";
    let forged_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{recipient}/messages/{forged_uuid}"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        forged_resp.status() == 404,
        "forged message_uuid must NOT verify — got {}",
        forged_resp.status()
    );

    let laundered_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{bystander}/messages/{message_uuid}"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        laundered_resp.status() == 404,
        "a real row addressed to a DIFFERENT session must not verify for the bystander — got {}",
        laundered_resp.status()
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
