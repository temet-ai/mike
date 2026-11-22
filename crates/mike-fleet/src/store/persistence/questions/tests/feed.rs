use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use sqlx::postgres::PgListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::seed::{fixture, question, session};
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;

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

#[tokio::test]
async fn open_questions_with_a_fed_feed_keep_an_empty_answer_until_the_recipient_is_asked()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let other = PersistHandle::connect(&f.db.database_url).await?;
    let feed = fed(&f.handle);
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    assert!(
        f.handle
            .launch()
            .open_questions_for(recipient)
            .await?
            .is_empty()
    );

    other
        .launch()
        .ask_question(&question(asker, recipient, "which resolver?"))
        .await?;
    assert!(
        f.handle
            .launch()
            .open_questions_for(recipient)
            .await?
            .is_empty()
    );

    feed.record(
        RowChangedTable::WorkspaceQuestions,
        Some(&asker.to_string()),
    );
    assert!(
        f.handle
            .launch()
            .open_questions_for(recipient)
            .await?
            .is_empty()
    );

    feed.record(
        RowChangedTable::WorkspaceQuestions,
        Some(&recipient.to_string()),
    );
    assert_eq!(
        f.handle.launch().open_questions_for(recipient).await?.len(),
        1
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn open_questions_without_a_fed_feed_are_read_every_time() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    assert!(
        f.handle
            .launch()
            .open_questions_for(recipient)
            .await?
            .is_empty()
    );
    f.handle
        .launch()
        .ask_question(&question(asker, recipient, "which resolver?"))
        .await?;
    assert_eq!(
        f.handle.launch().open_questions_for(recipient).await?.len(),
        1
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn asking_a_question_notifies_the_change_feed_with_the_recipient() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    let mut listener = PgListener::connect(&f.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    f.handle
        .launch()
        .ask_question(&question(asker, recipient, "which resolver?"))
        .await?;
    let payload: serde_json::Value = loop {
        let note = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await??;
        let payload: serde_json::Value = serde_json::from_str(note.payload())?;
        if payload["table"] == "workspace_questions" {
            break payload;
        }
    };
    assert_eq!(payload["session_id"], recipient.to_string());
    assert_eq!(payload["op"], "INSERT");
    f.db.cleanup().await?;
    Ok(())
}
