use crate::store::persistence::launch_repo::LaunchPersist as _;

use std::collections::BTreeSet;
use std::time::Duration;

use sqlx::postgres::PgListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

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

async fn lists(handle: &PersistHandle, session: Uuid) -> anyhow::Result<(usize, usize)> {
    let mail = handle
        .launch()
        .fetch_for_session_with_activity(session)
        .await?;
    Ok((mail.inbox.len(), mail.outbox.len()))
}

#[tokio::test]
async fn a_sessions_mail_lists_are_read_again_only_after_its_mailbox_moves() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&here);
    let sender = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    for session in [sender, recipient] {
        here.launch()
            .ensure_session_exists(&session.to_string())
            .await?;
    }
    assert_eq!(lists(&here, sender).await?, (0, 0));

    other
        .launch()
        .insert_message(Uuid::new_v4(), recipient, Some(sender), "first")
        .await?
        .ok_or_else(|| anyhow::anyhow!("mail was not queued"))?;
    assert_eq!(
        lists(&here, sender).await?,
        (0, 0),
        "the send has not reached the feed"
    );

    feed.record(RowChangedTable::MailboxMessages, Some(&sender.to_string()));
    assert_eq!(lists(&here, sender).await?, (0, 1));
    assert_eq!(lists(&here, recipient).await?, (1, 0));
    db.cleanup().await
}

#[tokio::test]
async fn a_mailbox_change_notice_names_the_recipient_and_the_sender() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let sender = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    for session in [sender, recipient] {
        handle
            .launch()
            .ensure_session_exists(&session.to_string())
            .await?;
    }
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    let message = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message, recipient, Some(sender), "hello")
        .await?
        .ok_or_else(|| anyhow::anyhow!("mail was not queued"))?;

    let mut named = BTreeSet::new();
    while named.len() < 2 {
        let notice = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["table"] == "mailbox_messages" && payload["id"] == message.to_string().as_str() {
            named.insert(payload["session_id"].as_str().unwrap_or_default().to_owned());
        }
    }
    assert_eq!(
        named,
        BTreeSet::from([sender.to_string(), recipient.to_string()])
    );
    db.cleanup().await
}
