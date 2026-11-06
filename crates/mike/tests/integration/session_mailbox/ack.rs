use crate::seed;
use crate::test_db;

use super::support::{AckOutcome, ack_message, fetch_pending_unclaimed, insert_message};
use seed::{seed_project, seed_session};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn ack_unknown_message_returns_not_found() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/mailbox-ack-unknown-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let to_uuid = uuid::Uuid::new_v4();
    seed_session(&pool, project_id, &to_uuid.to_string()).await?;

    let result = ack_message(&pool, to_uuid, uuid::Uuid::new_v4()).await?;
    assert_eq!(
        result,
        AckOutcome::NotFound,
        "acking an unknown message must return NotFound (handler → 404)"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn ack_never_delivered_message_is_refused() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/mailbox-ack-never-delivered-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let to_uuid = uuid::Uuid::new_v4();
    seed_session(&pool, project_id, &to_uuid.to_string()).await?;

    let msg = uuid::Uuid::new_v4();
    insert_message(&pool, msg, to_uuid, None, "never delivered").await?;

    let outcome = ack_message(&pool, to_uuid, msg).await?;
    assert_eq!(outcome, AckOutcome::NeverDelivered);

    assert_eq!(fetch_pending_unclaimed(&pool, to_uuid).await?, vec![msg]);

    handle.cleanup().await?;
    Ok(())
}
