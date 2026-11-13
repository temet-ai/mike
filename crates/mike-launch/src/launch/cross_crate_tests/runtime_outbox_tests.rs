use crate::store::persistence::runtime_repo::RuntimePersist as _;

use crate::store::persistence::runtime::tests_observation_support::fixture;

#[tokio::test]
async fn observation_outbox_rotates_unavailable_backlog_and_settles_only_mailbox_receipts()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    sqlx::query!(
        "WITH originals AS (
        INSERT INTO mailbox.messages (message_uuid, to_session_id, body)
        SELECT gen_random_uuid(), $1, 'backlog fixture' FROM generate_series(1, 105)
        RETURNING message_uuid, body)
        INSERT INTO mailbox.outbox (message_uuid, to_session, runtime, body, generation)
        SELECT message_uuid, $2, 'codex', body, 0 FROM originals",
        f.session_db,
        f.session
    )
    .execute(f.handle.pool())
    .await?;
    let first = sqlx::query_file_scalar!("src/mail_outbox/open_messages.sql", 100_i64)
        .fetch_all(f.handle.pool())
        .await?;
    let second = sqlx::query_file_scalar!("src/mail_outbox/open_messages.sql", 100_i64)
        .fetch_all(f.handle.pool())
        .await?;
    assert_eq!(first.len(), 100);
    assert_eq!(second.iter().filter(|id| !first.contains(id)).count(), 5);
    let queued = first[0];
    assert!(
        f.handle
            .runtime()
            .reserve_runtime_mail_notification(f.session, queued)
            .await?
    );
    f.handle
        .runtime()
        .finish_runtime_mail_notification(queued, "queued", None)
        .await?;
    assert_eq!(f.handle.runtime().reconcile_runtime_mail_outbox().await?, 0);
    let available = sqlx::query_file_scalar!("src/mail_outbox/open_messages.sql", 200_i64)
        .fetch_all(f.handle.pool())
        .await?;
    assert!(!available.contains(&queued));
    sqlx::query!(
        "UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1",
        queued
    )
    .execute(f.handle.pool())
    .await?;
    assert_eq!(f.handle.runtime().reconcile_runtime_mail_outbox().await?, 1);
    let status = sqlx::query_scalar!(
        "SELECT status FROM mailbox.outbox WHERE message_uuid = $1",
        queued
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(status, "delivered");
    let consumed = sqlx::query_scalar!(
        "SELECT stage FROM mailbox.outbox_stages WHERE message_uuid = $1 AND stage = 'consumed'",
        queued
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(consumed, "consumed");
    f.cleanup().await
}
