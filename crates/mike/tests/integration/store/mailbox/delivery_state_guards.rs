use chrono::{DateTime, Utc};

use mike::store::persistence::PersistHandle;
use mike::store::persistence::mailbox::AckOutcome;

use super::support::seed_session;
use crate::test_db::TestDb;

#[tokio::test]
async fn insert_failed_send_resolves_attempted_by_and_allows_null() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        format!("proj-failed-send-test-{}", uuid::Uuid::new_v4()),
    )
    .fetch_one(&pool)
    .await?;

    let caller_uuid = uuid::Uuid::new_v4();
    let caller_session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        caller_uuid,
        project_id,
    )
    .fetch_one(&pool)
    .await?;

    let bogus_recipient = uuid::Uuid::new_v4().to_string();
    let id_known_caller = handle
        .insert_failed_send(
            Some(caller_uuid),
            &bogus_recipient,
            "session_not_found",
            None,
        )
        .await?;
    let resolved_caller: Option<i64> = sqlx::query_scalar!(
        "SELECT attempted_by_session_id FROM mailbox.failed_sends WHERE id = $1",
        id_known_caller,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        resolved_caller,
        Some(caller_session_id),
        "known attempted_by_session_uuid resolves to its bigint id"
    );

    let id_no_caller = handle
        .insert_failed_send(None, &bogus_recipient, "session_not_found", None)
        .await?;
    let resolved_none: Option<i64> = sqlx::query_scalar!(
        "SELECT attempted_by_session_id FROM mailbox.failed_sends WHERE id = $1",
        id_no_caller,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(resolved_none, None, "absent caller persists as NULL");

    let unknown_uuid = uuid::Uuid::new_v4();
    let id_unknown_caller = handle
        .insert_failed_send(
            Some(unknown_uuid),
            &bogus_recipient,
            "invalid_from_session_id",
            None,
        )
        .await?;
    let resolved_unknown: Option<i64> = sqlx::query_scalar!(
        "SELECT attempted_by_session_id FROM mailbox.failed_sends WHERE id = $1",
        id_unknown_caller,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        resolved_unknown, None,
        "unresolvable caller uuid persists as NULL, not an error"
    );

    let rejected = handle
        .insert_failed_send(None, &bogus_recipient, "not_a_real_reason", None)
        .await;
    assert!(
        rejected.is_err(),
        "reason outside the CHECK vocabulary must be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn has_pending_inbound_mail_reflects_claim_state() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        format!("proj-mailbox-pending-gate-{}", uuid::Uuid::new_v4()),
    )
    .fetch_one(&pool)
    .await?;

    let to_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW())",
        to_uuid,
        project_id,
    )
    .execute(&pool)
    .await?;

    assert!(
        !handle.has_pending_inbound_mail(to_uuid).await?,
        "no mailbox rows exist yet"
    );

    let msg = uuid::Uuid::new_v4();
    handle
        .insert_message(msg, to_uuid, None, "check the mailbox before claiming done")
        .await?
        .ok_or_else(|| anyhow::anyhow!("insert_message returned None for a known session"))?;
    assert!(
        handle.has_pending_inbound_mail(to_uuid).await?,
        "a fresh unclaimed row is pending"
    );

    handle.claim_message(msg).await?;
    assert!(
        !handle.has_pending_inbound_mail(to_uuid).await?,
        "a claimed row is no longer pending"
    );

    Ok(())
}

#[tokio::test]
async fn ack_message_refuses_undelivered_row_then_succeeds_once_delivered() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let to_uuid = seed_session(
        &pool,
        &format!("proj-ack-requires-delivery-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let msg = uuid::Uuid::new_v4();
    handle
        .insert_message(msg, to_uuid, None, "please ack me")
        .await?
        .ok_or_else(|| anyhow::anyhow!("insert_message returned None for a known session"))?;

    assert_eq!(
        handle.ack_message(to_uuid, msg).await?,
        AckOutcome::NeverDelivered
    );
    let consumed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT consumed_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(msg)
            .fetch_one(&pool)
            .await?;
    assert!(
        consumed_at.is_none(),
        "a refused ack must not set consumed_at"
    );

    handle.claim_message(msg).await?;
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
                 (session_id, project_id, method_id, path, is_streaming, message_count,
                  is_tool_loop, status_id, params, last_user_message, human_prompt,
                  turn_type_id, turn_origin_id, created_at)
             SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 1, false, rs.id,
                    '{}'::jsonb, '', '', rtt.id, rto.id, now()
               FROM sessions.sessions s,
                    requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE s.session_uuid = $1
                AND rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = 'primary'
             RETURNING id",
    )
    .bind(to_uuid)
    .fetch_one(&pool)
    .await?;
    handle
        .backfill_mailbox_sent_request_id(msg, request_id)
        .await?;

    let outcome = handle.ack_message(to_uuid, msg).await?;
    let AckOutcome::Consumed(first_consumed_at) = outcome else {
        anyhow::bail!("expected Consumed after delivery, got {outcome:?}");
    };

    let outcome = handle.ack_message(to_uuid, msg).await?;
    assert_eq!(outcome, AckOutcome::Consumed(first_consumed_at));

    let unknown_pair_to_uuid = seed_session(
        &pool,
        &format!("proj-ack-unknown-pair-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    assert_eq!(
        handle
            .ack_message(unknown_pair_to_uuid, uuid::Uuid::new_v4())
            .await?,
        AckOutcome::NotFound
    );

    Ok(())
}

#[tokio::test]
async fn claim_message_refuses_dead_lettered_and_consumed_rows() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let to_uuid = seed_session(
        &pool,
        &format!("proj-claim-refuses-dead-lettered-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let msg = uuid::Uuid::new_v4();
    handle
        .insert_message(msg, to_uuid, None, "never delivered, then reaped")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET dead_lettered_at = now() WHERE message_uuid = $1",
        msg,
    )
    .execute(&pool)
    .await?;

    assert!(
        !handle.claim_message(msg).await?,
        "a dead-lettered row must never be claimable"
    );
    let claimed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT claimed_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(msg)
            .fetch_one(&pool)
            .await?;
    assert!(
        claimed_at.is_none(),
        "a refused claim must not stamp claimed_at"
    );

    let to_uuid2 = seed_session(
        &pool,
        &format!("proj-claim-refuses-consumed-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let msg2 = uuid::Uuid::new_v4();
    handle
        .insert_message(msg2, to_uuid2, None, "consumed before ever being claimed")
        .await?;
    sqlx::query!(
        "UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1",
        msg2,
    )
    .execute(&pool)
    .await?;

    assert!(
        !handle.claim_message(msg2).await?,
        "a consumed row must never be claimable"
    );

    Ok(())
}

#[tokio::test]
async fn unclaim_message_refuses_a_row_dead_lettered_since_the_claim() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let to_uuid = seed_session(
        &pool,
        &format!("proj-unclaim-matching-invariant-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let msg = uuid::Uuid::new_v4();
    handle
        .insert_message(msg, to_uuid, None, "claimed then reaped mid-flight")
        .await?;
    assert!(handle.claim_message(msg).await?, "first claim succeeds");
    sqlx::query!(
        "UPDATE mailbox.messages SET dead_lettered_at = now() WHERE message_uuid = $1",
        msg,
    )
    .execute(&pool)
    .await?;

    assert!(
        !handle.unclaim_message(msg).await?,
        "unclaim must refuse a row dead-lettered since the claim"
    );
    let claimed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT claimed_at FROM mailbox.messages WHERE message_uuid = $1")
            .bind(msg)
            .fetch_one(&pool)
            .await?;
    assert!(
        claimed_at.is_some(),
        "a refused unclaim must leave claimed_at untouched — never re-expose a \
         reaped row as pending"
    );

    Ok(())
}

#[tokio::test]
async fn is_first_mail_for_recipient_tracks_per_recipient_ordering() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let recipient_a = seed_session(
        &pool,
        &format!("proj-first-mail-recipient-a-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let recipient_b = seed_session(
        &pool,
        &format!("proj-first-mail-recipient-b-{}", uuid::Uuid::new_v4()),
    )
    .await?;

    let first_for_a = uuid::Uuid::new_v4();
    handle
        .insert_message(first_for_a, recipient_a, None, "first to a")
        .await?;
    assert!(
        handle.is_first_mail_for_recipient(first_for_a).await?,
        "a recipient's first-ever row is first"
    );

    let second_for_a = uuid::Uuid::new_v4();
    handle
        .insert_message(second_for_a, recipient_a, None, "second to a")
        .await?;
    assert!(
        !handle.is_first_mail_for_recipient(second_for_a).await?,
        "a second row for the same recipient is not first"
    );
    assert!(
        handle.is_first_mail_for_recipient(first_for_a).await?,
        "the first row for a recipient stays first after a second arrives"
    );

    let first_for_b = uuid::Uuid::new_v4();
    handle
        .insert_message(first_for_b, recipient_b, None, "first to b")
        .await?;
    assert!(
        handle.is_first_mail_for_recipient(first_for_b).await?,
        "a different recipient's own first row is unaffected by recipient a's history"
    );

    Ok(())
}
