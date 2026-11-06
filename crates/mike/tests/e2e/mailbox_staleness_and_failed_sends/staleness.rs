use serde_json::json;
use sqlx::postgres::PgPoolOptions;

use super::support::{TestDb, boot_proxy, get_messages, queue_message, seed_session};

#[tokio::test]
async fn mailbox_message_reports_staleness_and_flips_on_backdate() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping mailbox_message_reports_staleness_and_flips_on_backdate"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;

    let message_uuid = queue_message(&client, &inspect_url, session, "check in on this").await?;

    let body = get_messages(&client, &inspect_url, session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["message_uuid"], message_uuid.to_string());
    assert_eq!(row["status"], "pending");
    let pending_minutes = row["pending_minutes"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("pending_minutes missing on a pending row: {row:?}"))?;
    assert!(
        (0..10).contains(&pending_minutes),
        "a freshly queued row should be well under the threshold: {pending_minutes}"
    );
    assert_eq!(
        row["stale"],
        json!(false),
        "a freshly queued row must not be stale yet: {row:?}"
    );

    sqlx::query("UPDATE mailbox.messages SET queued_at = now() - INTERVAL '15 minutes' WHERE message_uuid = $1")
        .bind(message_uuid)
        .execute(&pool)
        .await?;

    let body = get_messages(&client, &inspect_url, session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["status"], "pending");
    let pending_minutes = row["pending_minutes"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("pending_minutes missing after backdate: {row:?}"))?;
    assert!(
        pending_minutes >= 10,
        "backdated row must report elapsed minutes past the threshold: {pending_minutes}"
    );
    assert_eq!(
        row["stale"],
        json!(true),
        "a row pending past the threshold must flip to stale: {row:?}"
    );

    sqlx::query("UPDATE mailbox.messages SET claimed_at = now() WHERE message_uuid = $1")
        .bind(message_uuid)
        .execute(&pool)
        .await?;

    let body = get_messages(&client, &inspect_url, session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["status"], "claimed");
    assert!(
        row["pending_minutes"].is_null(),
        "a claimed row must not report pending_minutes: {row:?}"
    );
    assert!(
        row["stale"].is_null(),
        "a claimed row must not report stale: {row:?}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
