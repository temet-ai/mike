use crate::test_db;

use super::support::{
    AckOutcome, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS, ack_message,
    bump_redelivery_attempt, redelivery_state, seed_sent_message,
};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use test_db::TestDb;

#[tokio::test]
async fn redelivery_eligible_immediately_after_first_send() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let to_uuid = uuid::Uuid::new_v4();
    let msg = seed_sent_message(
        &pool,
        &format!("/proj/mailbox-redelivery-1-{}", uuid::Uuid::new_v4()),
        to_uuid,
    )
    .await?;

    let (attempts, consumed, dead_lettered) = redelivery_state(&pool, msg).await?;
    assert_eq!(attempts, 1, "backfill counts the first send as attempt 1");
    assert!(!consumed);
    assert!(!dead_lettered);

    let outcome =
        bump_redelivery_attempt(&pool, msg, 5, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS)
            .await?;
    assert_eq!(
        outcome,
        Some((2, false, true)),
        "redelivery bumps attempts to 2, well under the cap of 5, and this bump was attempted"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn consumed_message_is_not_redelivery_eligible() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let to_uuid = uuid::Uuid::new_v4();
    let msg = seed_sent_message(
        &pool,
        &format!("/proj/mailbox-redelivery-2-{}", uuid::Uuid::new_v4()),
        to_uuid,
    )
    .await?;

    let outcome = ack_message(&pool, to_uuid, msg).await?;
    let AckOutcome::Consumed(consumed_at) = outcome else {
        anyhow::bail!("ack must succeed for a delivered session+message, got {outcome:?}");
    };

    let outcome =
        bump_redelivery_attempt(&pool, msg, 5, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS)
            .await?;
    assert_eq!(
        outcome, None,
        "a consumed row must not be redelivery-eligible"
    );
    let (attempts, consumed, _dead_lettered) = redelivery_state(&pool, msg).await?;
    assert_eq!(
        attempts, 1,
        "delivery_attempts unchanged by the failed bump"
    );
    assert!(consumed);

    let repeat = ack_message(&pool, to_uuid, msg).await?;
    assert_eq!(
        repeat,
        AckOutcome::Consumed(consumed_at),
        "repeat ack returns the ORIGINAL consumed_at, not a fresh one"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn redelivery_cap_reached_dead_letters_and_stops_further_bumps() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let to_uuid = uuid::Uuid::new_v4();
    let msg = seed_sent_message(
        &pool,
        &format!("/proj/mailbox-redelivery-cap-{}", uuid::Uuid::new_v4()),
        to_uuid,
    )
    .await?;

    let first_bump =
        bump_redelivery_attempt(&pool, msg, 3, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS)
            .await?;
    assert_eq!(
        first_bump,
        Some((2, false, true)),
        "attempt 2, cap not yet reached, and this bump was attempted"
    );

    let too_soon =
        bump_redelivery_attempt(&pool, msg, 3, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS)
            .await?;
    assert_eq!(
        too_soon,
        Some((2, false, false)),
        "a bump called before the backoff window elapses must be deferred, not counted"
    );
    let (attempts, _consumed, dead_lettered) = redelivery_state(&pool, msg).await?;
    assert_eq!(
        attempts, 2,
        "the deferred bump must not have advanced delivery_attempts"
    );
    assert!(!dead_lettered);

    tokio::time::sleep(Duration::from_millis(
        (TEST_BACKOFF_BASE_SECS * 1_000.0) as u64 + 100,
    ))
    .await;

    let second_bump =
        bump_redelivery_attempt(&pool, msg, 3, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS)
            .await?;
    assert_eq!(
        second_bump,
        Some((3, true, true)),
        "attempt 3 reaches the cap once the backoff window elapses — this bump IS the one that dead-letters"
    );
    let (attempts, consumed, dead_lettered) = redelivery_state(&pool, msg).await?;
    assert_eq!(attempts, 3);
    assert!(!consumed);
    assert!(dead_lettered, "dead_lettered_at must be stamped at the cap");

    let third_bump =
        bump_redelivery_attempt(&pool, msg, 3, TEST_BACKOFF_BASE_SECS, TEST_BACKOFF_CAP_SECS)
            .await?;
    assert_eq!(
        third_bump, None,
        "a dead-lettered row must not accept further redelivery bumps"
    );
    let (attempts_after, _consumed, _dead_lettered) = redelivery_state(&pool, msg).await?;
    assert_eq!(
        attempts_after, 3,
        "a rejected bump must not increment delivery_attempts"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rapid_redelivery_calls_within_backoff_window_do_not_exhaust_the_budget()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let to_uuid = uuid::Uuid::new_v4();
    let msg = seed_sent_message(
        &pool,
        &format!("/proj/mailbox-redelivery-rapid-{}", uuid::Uuid::new_v4()),
        to_uuid,
    )
    .await?;

    const RAPID_BACKOFF_BASE_SECS: f64 = 600.0;
    const RAPID_BACKOFF_CAP_SECS: f64 = 600.0;
    let mut attempted_count = 0;
    for _ in 0..5 {
        let Some(outcome) = bump_redelivery_attempt(
            &pool,
            msg,
            5,
            RAPID_BACKOFF_BASE_SECS,
            RAPID_BACKOFF_CAP_SECS,
        )
        .await?
        else {
            anyhow::bail!(
                "row must stay redelivery-eligible throughout this loop — never consumed or dead-lettered here"
            );
        };
        if outcome.2 {
            attempted_count += 1;
        }
    }

    assert_eq!(
        attempted_count, 1,
        "five rapid calls inside one backoff window must count as exactly one real attempt"
    );
    let (attempts, _consumed, dead_lettered) = redelivery_state(&pool, msg).await?;
    assert_eq!(
        attempts, 2,
        "delivery_attempts must have advanced only once (1 -> 2), not five times"
    );
    assert!(
        !dead_lettered,
        "a message must not be dead-lettered by rapid re-attempts within a single backoff window"
    );

    handle.cleanup().await?;
    Ok(())
}
