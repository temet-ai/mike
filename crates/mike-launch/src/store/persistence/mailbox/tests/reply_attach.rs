use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn session(handle: &PersistHandle) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    handle
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    Ok(session)
}

async fn mail(handle: &PersistHandle, to: Uuid, from: Uuid) -> anyhow::Result<Uuid> {
    let message = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message, to, Some(from), "mail")
        .await?
        .ok_or_else(|| anyhow::anyhow!("mail was not queued"))?;
    Ok(message)
}

async fn links(
    handle: &PersistHandle,
    message: Uuid,
) -> anyhow::Result<(Option<i64>, Option<i64>)> {
    Ok(sqlx::query_as(
        "SELECT thread_id, in_reply_to_id FROM mailbox.messages WHERE message_uuid = $1",
    )
    .bind(message)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn attach_reply_to_an_unthreaded_parent_threads_both_and_links_the_reply_once()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (asker, replier) = (session(&handle).await?, session(&handle).await?);
    let parent = mail(&handle, replier, asker).await?;
    let reply = mail(&handle, asker, replier).await?;

    assert!(
        handle
            .launch()
            .attach_reply(reply, parent, asker, replier)
            .await?
    );
    let (parent_thread, _) = links(&handle, parent).await?;
    let (reply_thread, reply_to) = links(&handle, reply).await?;
    let parent_id: i64 =
        sqlx::query_scalar("SELECT id FROM mailbox.messages WHERE message_uuid = $1")
            .bind(parent)
            .fetch_one(handle.pool())
            .await?;
    assert!(parent_thread.is_some());
    assert_eq!(reply_thread, parent_thread);
    assert_eq!(reply_to, Some(parent_id));
    assert!(
        !handle
            .launch()
            .attach_reply(reply, parent, asker, replier)
            .await?,
        "an attached reply is left alone"
    );
    db.cleanup().await
}
