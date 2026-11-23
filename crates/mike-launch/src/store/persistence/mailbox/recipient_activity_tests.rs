use crate::store::persistence::launch_repo::LaunchPersist as _;

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::postgres::PgListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::super::types::RecipientActivity;
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        CancellationToken::new(),
        &TaskTracker::new(),
    );
    feed
}

async fn session(handle: &PersistHandle) -> anyhow::Result<(Uuid, i64)> {
    let uuid = Uuid::new_v4();
    let id = handle
        .launch()
        .ensure_session_exists(&uuid.to_string())
        .await?;
    Ok((uuid, id))
}

async fn request_at(
    handle: &PersistHandle,
    session_id: i64,
    origin: &str,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let project = seed_project(handle.pool(), &format!("/activity/{}", Uuid::new_v4())).await?;
    sqlx::query(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, $4, $4
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = $3",
    )
    .bind(session_id)
    .bind(project)
    .bind(origin)
    .bind(at)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn activity_of(
    handle: &PersistHandle,
    recipient: (Uuid, i64),
) -> anyhow::Result<RecipientActivity> {
    Ok(handle
        .launch()
        .recipient_activity(&[recipient])
        .await?
        .remove(&recipient.1)
        .unwrap_or_default())
}

fn close(got: Option<DateTime<Utc>>, want: DateTime<Utc>) -> bool {
    got.is_some_and(|t| (t - want).num_milliseconds().abs() < 5)
}

#[tokio::test]
async fn recipient_activity_unmoved_requests_generation_keeps_the_held_answer() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let recipient = session(&here).await?;
    let first = Utc::now() - chrono::Duration::minutes(10);
    request_at(&other, recipient.1, "primary", first).await?;
    let held = activity_of(&here, recipient).await?;
    assert!(close(held.last_request_at, first));
    assert!(close(held.last_eligible_turn_at, first));

    let later = Utc::now() - chrono::Duration::minutes(1);
    request_at(&other, recipient.1, "subagent", later).await?;
    assert_eq!(
        activity_of(&here, recipient).await?,
        held,
        "the request has not reached the feed"
    );

    feed.record(RowChangedTable::Requests, Some(&recipient.0.to_string()));
    let moved = activity_of(&here, recipient).await?;
    assert!(close(moved.last_request_at, later));
    assert!(close(moved.last_eligible_turn_at, first));
    db.cleanup().await
}

#[tokio::test]
async fn session_mailbox_delivered_mail_carries_no_recipient_activity() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let sender = session(&handle).await?;
    let waiting = session(&handle).await?;
    let delivered = session(&handle).await?;
    let at = Utc::now() - chrono::Duration::minutes(3);
    for recipient in [waiting, delivered] {
        request_at(&handle, recipient.1, "primary", at).await?;
    }
    let to_waiting = Uuid::new_v4();
    let to_delivered = Uuid::new_v4();
    for (message, recipient) in [(to_waiting, waiting), (to_delivered, delivered)] {
        handle
            .launch()
            .insert_message(message, recipient.0, Some(sender.0), "hello")
            .await?
            .ok_or_else(|| anyhow::anyhow!("mail was not queued"))?;
    }
    sqlx::query("UPDATE mailbox.messages SET channel_delivered_at = now() WHERE message_uuid = $1")
        .bind(to_delivered)
        .execute(handle.pool())
        .await?;

    let mail = handle
        .launch()
        .fetch_for_session_with_activity(sender.0)
        .await?;
    let row = |uuid: Uuid| {
        mail.outbox
            .iter()
            .find(|m| m.row.message_uuid == uuid)
            .ok_or_else(|| anyhow::anyhow!("outbox lacks {uuid}"))
    };
    let pending = row(to_waiting)?;
    assert!(close(pending.recipient_last_request_at, at));
    assert!(close(pending.recipient_last_eligible_turn_at, at));
    let done = row(to_delivered)?;
    assert_eq!(done.recipient_last_request_at, None);
    assert_eq!(done.recipient_last_eligible_turn_at, None);
    db.cleanup().await
}

#[tokio::test]
async fn recipient_activity_unknown_session_returns_none() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    assert_eq!(
        handle
            .launch()
            .fetch_recipient_activity(Uuid::new_v4())
            .await?,
        None
    );
    assert!(handle.launch().recipient_activity(&[]).await?.is_empty());
    db.cleanup().await
}

#[tokio::test]
async fn a_request_change_notice_names_its_session() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let recipient = session(&handle).await?;
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    request_at(&handle, recipient.1, "primary", Utc::now()).await?;

    loop {
        let notice = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["table"] == "requests" {
            assert_eq!(payload["session_id"], recipient.0.to_string().as_str());
            break;
        }
    }
    db.cleanup().await
}
