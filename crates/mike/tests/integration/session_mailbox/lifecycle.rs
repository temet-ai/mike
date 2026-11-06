use crate::seed;
use crate::test_db;

use super::support::{
    backfill_sent_request_id, claim_message, fetch_pending_unclaimed, insert_message, row_state,
    seed_request,
};
use seed::{seed_project, seed_session};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn lifecycle_insert_claim_backfill_no_double_deliver() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id =
        seed_project(&pool, &format!("/proj/mailbox-{}", uuid::Uuid::new_v4())).await?;
    let to_uuid = uuid::Uuid::new_v4();
    let from_uuid = uuid::Uuid::new_v4();
    seed_session(&pool, project_id, &to_uuid.to_string()).await?;
    seed_session(&pool, project_id, &from_uuid.to_string()).await?;
    let from_id: i64 = sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
        from_uuid
    )
    .fetch_one(&pool)
    .await?;
    let request_id = seed_request(&pool, project_id, to_uuid).await?;

    let msg = uuid::Uuid::new_v4();
    let inserted = insert_message(&pool, msg, to_uuid, Some(from_id), "hello over there").await?;
    assert_eq!(
        inserted,
        Some(msg),
        "insert must land for a known recipient"
    );
    let (claimed_at, sent) = row_state(&pool, msg).await?;
    assert!(
        claimed_at.is_none() && sent.is_none(),
        "fresh row is pending"
    );
    assert_eq!(
        fetch_pending_unclaimed(&pool, to_uuid).await?,
        vec![msg],
        "a pending row is in the reload set"
    );

    assert!(
        claim_message(&pool, msg).await?,
        "first claim transitions the row"
    );
    let (claimed_at, sent) = row_state(&pool, msg).await?;
    assert!(
        claimed_at.is_some() && sent.is_none(),
        "claimed but undelivered"
    );
    assert!(
        fetch_pending_unclaimed(&pool, to_uuid).await?.is_empty(),
        "RELOAD AFTER CLAIM MUST NOT RE-ENQUEUE — a restart between claim and \
         stamp cannot re-deliver"
    );

    assert!(
        !claim_message(&pool, msg).await?,
        "second claim on an already-claimed row affects zero rows"
    );

    assert!(
        backfill_sent_request_id(&pool, msg, request_id).await?,
        "backfill stamps the delivered row"
    );
    let (claimed_at, sent) = row_state(&pool, msg).await?;
    assert!(claimed_at.is_some(), "claimed_at retained through delivery");
    assert_eq!(
        sent,
        Some(request_id),
        "sent_request_id is the carrying request"
    );
    let sent_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar!(
        r#"SELECT sent_at AS "sent_at?" FROM mailbox.messages WHERE message_uuid = $1"#,
        msg,
    )
    .fetch_one(&pool)
    .await?;
    assert!(sent_at.is_some(), "sent_at stamped on delivery");
    assert!(
        fetch_pending_unclaimed(&pool, to_uuid).await?.is_empty(),
        "RELOAD AFTER DELIVERY MUST NOT RE-ENQUEUE"
    );

    assert!(
        !backfill_sent_request_id(&pool, msg, request_id).await?,
        "second backfill on a delivered row affects zero rows"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn insert_against_unknown_session_inserts_no_row() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let unknown = uuid::Uuid::new_v4();
    let msg = uuid::Uuid::new_v4();
    let inserted = insert_message(&pool, msg, unknown, None, "into the void").await?;
    assert_eq!(
        inserted, None,
        "insert against an unknown recipient inserts no row (handler → 404)"
    );

    let count: i64 = sqlx::query_scalar!(
        r#"SELECT count(*) AS "c!" FROM mailbox.messages WHERE message_uuid = $1"#,
        msg,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 0, "no mailbox row exists for this attempted message");

    handle.cleanup().await?;
    Ok(())
}
