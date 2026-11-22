use crate::store::persistence::launch_repo::LaunchPersist as _;

use uuid::Uuid;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

const DELIVER: &str =
    "UPDATE mailbox.messages SET channel_delivered_at = now() WHERE message_uuid = $1";

fn request_carrying(text: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": text}]}]
    }))
    .unwrap_or_default()
}

#[tokio::test]
async fn carried_mail_check_after_finding_nothing_delivered_answers_from_memory_until_the_mailbox_moves()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = ChangeFeed::new();
    here.attach_change_feed(feed.clone());
    let session = Uuid::new_v4();
    here.launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    let request = request_carrying("the brief text");
    assert!(
        here.launch()
            .consume_mail_carried_by(session, &request)
            .await?
            .is_empty()
    );

    let message = Uuid::new_v4();
    other
        .launch()
        .insert_message(message, session, None, "the brief text")
        .await?
        .ok_or_else(|| anyhow::anyhow!("mail was not queued"))?;
    sqlx::query(DELIVER)
        .bind(message)
        .execute(other.pool())
        .await?;
    assert!(
        here.launch()
            .consume_mail_carried_by(session, &request)
            .await?
            .is_empty(),
        "the delivery has not reached the feed"
    );

    feed.record(RowChangedTable::MailboxMessages, Some(&session.to_string()));
    assert_eq!(
        here.launch()
            .consume_mail_carried_by(session, &request)
            .await?,
        vec![message]
    );
    db.cleanup().await
}
