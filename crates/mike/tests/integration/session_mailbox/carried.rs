use crate::seed::{seed_project, seed_session};
use crate::test_db::TestDb;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

async fn consumed(handle: &PersistHandle, message: Uuid) -> anyhow::Result<bool> {
    let at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT consumed_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(message)
            .fetch_one(handle.pool())
            .await?;
    Ok(at.is_some())
}

fn request_carrying(texts: &[&str]) -> Vec<u8> {
    let content: Vec<serde_json::Value> = texts
        .iter()
        .map(|t| serde_json::json!({"type": "text", "text": t}))
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "messages": [{"role": "user", "content": content}]
    }))
    .unwrap_or_default()
}

#[tokio::test]
async fn a_delivered_message_is_consumed_by_the_next_request_that_carries_its_text()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let project_id = seed_project(pool, &format!("/proj/carried-{}", Uuid::new_v4())).await?;
    let recipient = Uuid::new_v4();
    seed_session(pool, project_id, &recipient.to_string()).await?;

    let carried = Uuid::new_v4();
    let unseen = Uuid::new_v4();
    let body = "T2: the check failed in \"commit.rs\"\nline two";
    handle
        .launch()
        .insert_message(carried, recipient, None, body)
        .await?;
    handle
        .launch()
        .insert_message(unseen, recipient, None, "never shown to the model")
        .await?;
    assert_eq!(
        handle
            .launch()
            .claim_channel_messages(recipient, 10)
            .await?
            .len(),
        2
    );

    let next_request = request_carrying(&["hello", body]);
    let consumed_now = handle
        .launch()
        .consume_mail_carried_by(recipient, &next_request)
        .await?;

    assert_eq!(consumed_now, vec![carried]);
    assert!(consumed(&handle, carried).await?);
    assert!(
        !consumed(&handle, unseen).await?,
        "a message whose text the request does not carry stays unconsumed and is delivered again"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mail_never_delivered_is_not_consumed_by_a_request_that_happens_to_carry_its_text()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let project_id = seed_project(
        pool,
        &format!("/proj/carried-undelivered-{}", Uuid::new_v4()),
    )
    .await?;
    let recipient = Uuid::new_v4();
    seed_session(pool, project_id, &recipient.to_string()).await?;

    let queued = Uuid::new_v4();
    handle
        .launch()
        .insert_message(queued, recipient, None, "queued but not delivered")
        .await?;

    let consumed_now = handle
        .launch()
        .consume_mail_carried_by(recipient, &request_carrying(&["queued but not delivered"]))
        .await?;

    assert!(consumed_now.is_empty());
    assert!(!consumed(&handle, queued).await?);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_request_of_another_session_does_not_consume_the_mail() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let project_id = seed_project(pool, &format!("/proj/carried-other-{}", Uuid::new_v4())).await?;
    let recipient = Uuid::new_v4();
    let other = Uuid::new_v4();
    seed_session(pool, project_id, &recipient.to_string()).await?;
    seed_session(pool, project_id, &other.to_string()).await?;

    let message = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message, recipient, None, "for the recipient only")
        .await?;
    assert_eq!(
        handle
            .launch()
            .claim_channel_messages(recipient, 10)
            .await?
            .len(),
        1
    );

    let consumed_now = handle
        .launch()
        .consume_mail_carried_by(other, &request_carrying(&["for the recipient only"]))
        .await?;

    assert!(consumed_now.is_empty());
    assert!(!consumed(&handle, message).await?);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
