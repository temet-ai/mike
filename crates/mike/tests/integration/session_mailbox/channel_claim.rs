use crate::seed::{seed_project, seed_session};
use crate::test_db::TestDb;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::mailbox::AckOutcome;
use uuid::Uuid;

#[tokio::test]
async fn claim_is_exactly_once_and_wins_over_in_band_delivery() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();

    let project_id = seed_project(pool, &format!("/proj/channel-claim-{}", Uuid::new_v4())).await?;
    let recipient = Uuid::new_v4();
    seed_session(pool, project_id, &recipient.to_string()).await?;
    let sender = Uuid::new_v4();
    seed_session(pool, project_id, &sender.to_string()).await?;

    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    handle
        .launch()
        .insert_message(first, recipient, Some(sender), "first message")
        .await?;
    handle
        .launch()
        .insert_message(second, recipient, Some(sender), "second message")
        .await?;

    let claimed = handle
        .launch()
        .claim_channel_messages(recipient, 10)
        .await?;
    assert_eq!(claimed.len(), 2, "both pending messages must be claimed");
    assert_eq!(claimed[0].message_uuid, first);
    assert_eq!(claimed[1].message_uuid, second);
    assert!(
        claimed[0].first_for_recipient,
        "the first row ever queued for this recipient must be flagged as first"
    );
    assert!(!claimed[1].first_for_recipient);
    assert_eq!(claimed[0].from_session_uuid, Some(sender));
    let envelope = claimed[0].envelope();
    assert!(
        envelope.contains("first message"),
        "the rendered envelope must carry the body: {envelope}"
    );

    let reclaimed = handle
        .launch()
        .claim_channel_messages(recipient, 10)
        .await?;
    assert!(
        reclaimed.is_empty(),
        "a second poll must not re-deliver messages the channel already claimed"
    );

    assert!(
        !handle.launch().claim_message(first).await?,
        "the in-band next-request injection path must not be able to claim a message the \
         channel already delivered — this is the delivery-preference guarantee"
    );
    assert!(
        !handle
            .launch()
            .backfill_mailbox_sent_request_id(first, i64::MAX)
            .await?,
        "in-band attribution backfill must also be refused for a channel-delivered message"
    );

    let ack = handle.launch().ack_message(recipient, first).await?;
    assert!(
        matches!(ack, AckOutcome::Consumed(_)),
        "ack must accept a channel-delivered message even though sent_request_id stayed NULL: {ack:?}"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_claims_never_double_deliver_the_same_message() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();

    let project_id = seed_project(
        pool,
        &format!("/proj/channel-claim-race-{}", Uuid::new_v4()),
    )
    .await?;
    let recipient = Uuid::new_v4();
    seed_session(pool, project_id, &recipient.to_string()).await?;
    let message_uuid = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message_uuid, recipient, None, "racy message")
        .await?;

    let (a, b) = tokio::join!(
        handle.launch().claim_channel_messages(recipient, 10),
        handle.launch().claim_channel_messages(recipient, 10),
    );
    let total_claimed = a?.len() + b?.len();
    assert_eq!(
        total_claimed, 1,
        "exactly one of the two concurrent pollers must claim the single pending message"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn released_mail_is_claimable_again() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();

    let project_id =
        seed_project(pool, &format!("/proj/channel-release-{}", Uuid::new_v4())).await?;
    let recipient = Uuid::new_v4();
    seed_session(pool, project_id, &recipient.to_string()).await?;
    let message_uuid = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message_uuid, recipient, None, "unsent message")
        .await?;

    assert_eq!(
        handle
            .launch()
            .claim_channel_messages(recipient, 10)
            .await?
            .len(),
        1
    );
    let released = handle
        .launch()
        .release_channel_messages(recipient, vec![message_uuid])
        .await?;
    assert_eq!(released, 1, "the claimed row must be released");
    assert!(
        handle.launch().claim_message(message_uuid).await?,
        "a released message must be claimable by the in-band injection path again"
    );
    assert_eq!(
        handle
            .launch()
            .release_channel_messages(Uuid::new_v4(), vec![message_uuid])
            .await?,
        0,
        "another session must not be able to release this recipient's mail"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
