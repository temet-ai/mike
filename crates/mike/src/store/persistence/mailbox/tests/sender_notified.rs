use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::{NewLaunch, NewRun};
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

const SENDER_NOTIFIED_MIGRATION: &str =
    include_str!("../../migrations/20261102360000_mailbox_messages_sender_notified_at.sql");

async fn fixture() -> anyhow::Result<Option<(TestDbHandle, PersistHandle, String)>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let run = format!("sender-notified-{}", Uuid::new_v4());
    handle
        .upsert_run(&NewRun {
            run_id: run.clone(),
            project_path: None,
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    Ok(Some((db, handle, run)))
}

async fn launch(
    handle: &PersistHandle,
    run: &str,
    session: Uuid,
    seat: &str,
) -> anyhow::Result<i64> {
    handle.ensure_session_exists(&session.to_string()).await?;
    let mut new = NewLaunch::new("herdr", "/tmp/sender-notified-test");
    new.run_id = Some(run.to_owned());
    new.session_uuid = Some(session);
    new.seat = Some(seat.to_owned());
    Ok(handle.insert_launch(&new).await?)
}

async fn queue_aged(handle: &PersistHandle, recipient: Uuid, sender: Uuid) -> anyhow::Result<Uuid> {
    let message = Uuid::new_v4();
    handle
        .insert_message(message, recipient, Some(sender), "Waiting mail\nFull body")
        .await?
        .ok_or_else(|| anyhow::anyhow!("message to an open recipient was refused"))?;
    sqlx::query("UPDATE mailbox.messages SET queued_at = now() - interval '10 minutes' WHERE message_uuid = $1")
        .bind(message)
        .execute(handle.pool())
        .await?;
    Ok(message)
}

async fn notified_at(
    handle: &PersistHandle,
    message: Uuid,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    Ok(sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT sender_notified_at FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message)
    .fetch_one(handle.pool())
    .await?)
}

async fn notices_to(handle: &PersistHandle, sender: Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM mailbox.messages m
           JOIN sessions.sessions s ON s.id = m.to_session_id
          WHERE s.session_uuid = $1 AND m.body LIKE '[mike mail] Not delivered:%'",
    )
    .bind(sender)
    .fetch_one(handle.pool())
    .await?)
}

fn reaper_cutoff() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::minutes(5)
}

#[tokio::test]
async fn tell_inactive_senders_declined_notice_is_not_served_next_tick() -> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (sender, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    launch(&handle, &run, sender, "coordinator").await?;
    let recipient_launch = launch(&handle, &run, recipient, "worker-P1").await?;
    let message = queue_aged(&handle, recipient, sender).await?;
    handle
        .record_launch_closed(recipient_launch, "complete", Some(true))
        .await?;

    assert_eq!(handle.tell_inactive_senders(reaper_cutoff()).await?, 0);
    let settled = notified_at(&handle, message).await?;
    assert!(
        settled.is_some(),
        "a declined notice still settles the message"
    );
    assert_eq!(handle.tell_inactive_senders(reaper_cutoff()).await?, 0);
    assert_eq!(notified_at(&handle, message).await?, settled);
    assert_eq!(notices_to(&handle, sender).await?, 0);
    db.cleanup().await
}

#[tokio::test]
async fn tell_inactive_senders_notified_message_is_not_notified_again() -> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (sender, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    launch(&handle, &run, sender, "coordinator").await?;
    launch(&handle, &run, recipient, "worker-P1").await?;
    let message = queue_aged(&handle, recipient, sender).await?;

    assert_eq!(handle.tell_inactive_senders(reaper_cutoff()).await?, 1);
    let settled = notified_at(&handle, message).await?;
    assert!(settled.is_some());
    assert_eq!(handle.tell_inactive_senders(reaper_cutoff()).await?, 0);
    assert!(!handle.tell_sender_not_delivered(message).await?);
    handle
        .dead_letter_expired_undelivered(reaper_cutoff(), reaper_cutoff())
        .await?;
    assert_eq!(notified_at(&handle, message).await?, settled);
    assert_eq!(notices_to(&handle, sender).await?, 1);
    db.cleanup().await
}

#[tokio::test]
async fn backfill_message_with_existing_notice_is_stamped() -> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (sender, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    launch(&handle, &run, sender, "coordinator").await?;
    launch(&handle, &run, recipient, "worker-P1").await?;
    let told = queue_aged(&handle, recipient, sender).await?;
    let untold = queue_aged(&handle, recipient, sender).await?;
    let notice = Uuid::new_v4();
    handle
        .insert_message(
            notice,
            sender,
            None,
            &format!(
                "[mike mail] Not delivered: your message {told} to worker-P1 (session {recipient}) was never delivered: that session made no request in time.\nMessage first line: Waiting mail"
            ),
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("notice to the sender was refused"))?;
    let notice_queued_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT queued_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(notice)
            .fetch_one(handle.pool())
            .await?;

    sqlx::raw_sql(SENDER_NOTIFIED_MIGRATION)
        .execute(handle.pool())
        .await?;

    assert_eq!(notified_at(&handle, told).await?, Some(notice_queued_at));
    assert_eq!(notified_at(&handle, untold).await?, None);
    assert_eq!(handle.tell_inactive_senders(reaper_cutoff()).await?, 1);
    assert_eq!(notices_to(&handle, sender).await?, 2);
    db.cleanup().await
}
