use chrono::{DateTime, Utc};

use mike::store::persistence::PersistHandle;

use super::support::{seed_request_at, seed_session, seed_session_with_ids};
use crate::test_db::TestDb;

#[tokio::test]
async fn first_send_eligibility_excludes_consumed_and_dead_lettered_rows() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let to_uuid = seed_session(&pool, "proj-eligibility-excludes").await?;

    let pending_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(pending_msg, to_uuid, None, "still pending")
        .await?;

    let consumed_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(
            consumed_msg,
            to_uuid,
            None,
            "corrupted: consumed but never sent",
        )
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1",
        consumed_msg,
    )
    .execute(&pool)
    .await?;

    let dead_lettered_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(dead_lettered_msg, to_uuid, None, "already reaped")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET dead_lettered_at = now() WHERE message_uuid = $1",
        dead_lettered_msg,
    )
    .execute(&pool)
    .await?;

    let unclaimed = handle.fetch_pending_unclaimed(to_uuid, 100).await?;
    let unclaimed_uuids: Vec<_> = unclaimed.iter().map(|r| r.row.message_uuid).collect();
    assert_eq!(
        unclaimed_uuids,
        vec![pending_msg],
        "fetch_pending_unclaimed must return only the genuinely pending row"
    );

    let all_unclaimed = handle.fetch_all_pending_unclaimed(100).await?;
    assert!(
        all_unclaimed
            .iter()
            .all(|r| r.row.message_uuid == pending_msg),
        "fetch_all_pending_unclaimed must exclude the consumed and dead-lettered rows"
    );

    assert!(
        handle.has_pending_inbound_mail(to_uuid).await?,
        "the genuinely pending row still counts"
    );

    handle.claim_message(pending_msg).await?;
    assert!(
        !handle.has_pending_inbound_mail(to_uuid).await?,
        "consumed/dead-lettered rows must never count as pending inbound mail"
    );

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn dead_letter_expired_undelivered_reaps_only_the_expired_undelivered_row()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let to_uuid = seed_session(&pool, "proj-ttl-reaper").await?;

    let expired_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(expired_msg, to_uuid, None, "stranded past the TTL")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET queued_at = now() - INTERVAL '2 hours' WHERE message_uuid = $1",
        expired_msg,
    )
    .execute(&pool)
    .await?;

    let too_recent_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(too_recent_msg, to_uuid, None, "queued moments ago")
        .await?;

    let already_consumed_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(already_consumed_msg, to_uuid, None, "already consumed")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET queued_at = now() - INTERVAL '2 hours', consumed_at = now() WHERE message_uuid = $1",
        already_consumed_msg,
    )
    .execute(&pool)
    .await?;

    let already_dead_lettered_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(
            already_dead_lettered_msg,
            to_uuid,
            None,
            "already dead-lettered",
        )
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET queued_at = now() - INTERVAL '2 hours', dead_lettered_at = now() WHERE message_uuid = $1",
        already_dead_lettered_msg,
    )
    .execute(&pool)
    .await?;

    let cutoff = Utc::now() - chrono::Duration::minutes(60);
    let outcome = handle
        .dead_letter_expired_undelivered(cutoff, cutoff)
        .await?;
    assert_eq!(
        outcome.dead_lettered, 1,
        "only the one genuinely expired-and-undelivered row should be reaped"
    );

    let dead_lettered_now: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT dead_lettered_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(expired_msg)
            .fetch_one(&pool)
            .await?;
    assert!(dead_lettered_now.is_some());

    for untouched in [
        too_recent_msg,
        already_consumed_msg,
        already_dead_lettered_msg,
    ] {
        let row: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) = sqlx::query_as(
            "SELECT dead_lettered_at, consumed_at FROM mailbox.messages WHERE message_uuid = $1",
        )
        .bind(untouched)
        .fetch_one(&pool)
        .await?;
        if untouched == already_dead_lettered_msg {
            assert!(row.0.is_some());
        } else if untouched == already_consumed_msg {
            assert!(
                row.0.is_none(),
                "an already-consumed row must not be dead-lettered"
            );
        } else {
            assert!(row.0.is_none(), "a too-recent row must not be reaped yet");
        }
    }

    let failed_send_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mailbox.failed_sends WHERE attempted_to = $1 AND reason = 'strand_ttl_expired'",
    )
    .bind(to_uuid.to_string())
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        failed_send_count, 1,
        "exactly one strand_ttl_expired failed_sends row for the one reaped message"
    );

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_predicate_dead_letters_every_stranded_row_regardless_of_age() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let to_uuid = seed_session(&pool, "proj-backfill-predicate").await?;

    let fresh_stranded_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(
            fresh_stranded_msg,
            to_uuid,
            None,
            "stranded, queued moments ago",
        )
        .await?;

    let already_consumed_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(already_consumed_msg, to_uuid, None, "already consumed")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1",
        already_consumed_msg,
    )
    .execute(&pool)
    .await?;

    let far_future_cutoff = Utc::now() + chrono::Duration::days(3650);
    let outcome = handle
        .dead_letter_expired_undelivered(far_future_cutoff, far_future_cutoff)
        .await?;
    assert_eq!(
        outcome.dead_lettered, 1,
        "the backfill predicate must dead-letter the one stranded row, real count: {}",
        outcome.dead_lettered
    );

    let stranded_now: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT dead_lettered_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(fresh_stranded_msg)
            .fetch_one(&pool)
            .await?;
    assert!(stranded_now.is_some());

    let consumed_untouched: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT dead_lettered_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(already_consumed_msg)
            .fetch_one(&pool)
            .await?;
    assert!(
        consumed_untouched.is_none(),
        "the backfill predicate must never touch an already-consumed row"
    );

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn dead_letter_expired_undelivered_skips_recipient_with_activity_in_any_lane()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (busy_uuid, busy_id, busy_project_id) =
        seed_session_with_ids(&pool, "proj-reaper-busy-recipient").await?;
    let (silent_uuid, _silent_id, _silent_project_id) =
        seed_session_with_ids(&pool, "proj-reaper-silent-recipient").await?;

    let busy_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(busy_msg, busy_uuid, None, "queued for a busy recipient")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET queued_at = now() - INTERVAL '2 hours' WHERE message_uuid = $1",
        busy_msg,
    )
    .execute(&pool)
    .await?;
    seed_request_at(
        &pool,
        busy_id,
        busy_project_id,
        "subagent",
        Utc::now() - chrono::Duration::minutes(5),
    )
    .await?;

    let silent_msg = uuid::Uuid::new_v4();
    handle
        .insert_message(
            silent_msg,
            silent_uuid,
            None,
            "queued for a silent recipient",
        )
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET queued_at = now() - INTERVAL '2 hours' WHERE message_uuid = $1",
        silent_msg,
    )
    .execute(&pool)
    .await?;

    let cutoff = Utc::now() - chrono::Duration::minutes(60);
    let outcome = handle
        .dead_letter_expired_undelivered(cutoff, cutoff)
        .await?;
    assert_eq!(
        outcome.dead_lettered, 1,
        "only the silent recipient's row should be reaped"
    );

    let busy_dead_lettered: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT dead_lettered_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(busy_msg)
            .fetch_one(&pool)
            .await?;
    assert!(
        busy_dead_lettered.is_none(),
        "a session with subagent-lane traffic must not be dead-lettered for working"
    );

    let silent_dead_lettered: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT dead_lettered_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(silent_msg)
            .fetch_one(&pool)
            .await?;
    assert!(silent_dead_lettered.is_some());

    let failed_send_reason: Option<String> =
        sqlx::query_scalar("SELECT reason FROM mailbox.failed_sends WHERE attempted_to = $1")
            .bind(silent_uuid.to_string())
            .fetch_optional(&pool)
            .await?;
    assert_eq!(failed_send_reason.as_deref(), Some("strand_ttl_expired"));

    handle_db.cleanup().await?;
    Ok(())
}
