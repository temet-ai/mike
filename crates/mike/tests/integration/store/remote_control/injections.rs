use chrono::{SubsecRound, Utc};
use uuid::Uuid;

use mike::store::persistence::remote_control::PushSource;

use crate::store::remote_control::support::{
    call_at, seed_message_with_channel, seed_session, seed_steerer_intervention, shared_handle,
    unique,
};

#[tokio::test]
async fn reservation_prevents_turn_drain_and_failed_emission_releases_it() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, message, cse_id) =
        seed_message_with_channel(&handle, &unique("/tmp/rc-injection")).await?;
    let event = Uuid::new_v4();
    let source = PushSource::Mailbox {
        message_uuid: message,
    };
    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, Uuid::new_v4(), &source, event, b"frame")
            .await?
            .is_none()
    );
    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
            .await?
            .is_some()
    );
    assert!(
        !handle.claim_message(message).await?,
        "turn drain cannot claim a queued frame too"
    );
    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, Uuid::new_v4(), b"frame")
            .await?
            .is_none()
    );
    handle.finish_remote_control_injection(event, false).await?;
    assert!(
        handle.claim_message(message).await?,
        "failed emission must restore fallback"
    );
    let failed: bool = sqlx::query_scalar("SELECT failed_at IS NOT NULL AND emitted_at IS NULL FROM remote_control.injections WHERE event_id=$1")
        .bind(event).fetch_one(handle.pool()).await?;
    assert!(failed, "failed attempts retain honest provenance");
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn ack_before_completion_write_is_idempotent_and_stops_redelivery() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, message, cse_id) =
        seed_message_with_channel(&handle, &unique("/tmp/rc-injection")).await?;
    let event = Uuid::new_v4();
    let source = PushSource::Mailbox {
        message_uuid: message,
    };
    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
            .await?
            .is_some()
    );
    let call = handle
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events/delivery"))
        .await?;
    let received = Utc::now();
    handle
        .ack_remote_control_injections(&cse_id, &[event], received, call)
        .await?;
    handle.finish_remote_control_injection(event, true).await?;
    handle
        .ack_remote_control_injections(
            &cse_id,
            &[event],
            received + chrono::Duration::seconds(1),
            call,
        )
        .await?;
    let row = handle
        .fetch_message_for_recipient(message, session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("message missing"))?;
    assert_eq!(row.consumed_at, Some(received.trunc_subsecs(6)));
    assert_eq!(row.delivery_attempts, 1);
    assert!(row.sent_request_id.is_none());
    assert!(!handle.claim_message(message).await?);
    assert!(
        handle
            .fetch_pending_redelivery(session, 10)
            .await?
            .is_empty()
    );
    assert!(
        handle
            .bump_redelivery_attempt(message, 5, 30, 300)
            .await?
            .is_none()
    );
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn reserve_steerer_injection_stamps_pushed_at_and_blocks_double_push() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _session_id, intervention_uuid, cse_id) =
        seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();

    let reserved_at = handle
        .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
        .await?;
    assert!(
        reserved_at.is_some(),
        "first reservation must claim the intervention"
    );
    let (source_kind, source_id): (String, Option<String>) = sqlx::query_as(
        "SELECT source_kind, source_id FROM remote_control.injections WHERE event_id=$1",
    )
    .bind(event)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(source_kind, "steerer");
    assert_eq!(
        source_id.as_deref(),
        Some(intervention_uuid.to_string().as_str())
    );
    let intervention_pushed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT pushed_at FROM steering.interventions WHERE uuid=$1")
            .bind(intervention_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        intervention_pushed_at, reserved_at,
        "steering.interventions.pushed_at must equal the reservation's own returned timestamp"
    );

    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, Uuid::new_v4(), b"frame")
            .await?
            .is_none()
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn finish_steerer_injection_failed_emission_clears_pushed_at() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _session_id, intervention_uuid, cse_id) =
        seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
            .await?
            .is_some()
    );

    handle.finish_remote_control_injection(event, false).await?;

    let pushed: bool = sqlx::query_scalar(
        "SELECT pushed_at IS NOT NULL FROM steering.interventions WHERE uuid=$1",
    )
    .bind(intervention_uuid)
    .fetch_one(handle.pool())
    .await?;
    assert!(
        !pushed,
        "a failed emission must release the pushed_at claim"
    );

    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, Uuid::new_v4(), b"frame")
            .await?
            .is_some()
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn ack_steering_interventions_stamps_acked_at() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _session_id, intervention_uuid, cse_id) =
        seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    assert!(
        handle
            .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
            .await?
            .is_some()
    );
    handle.finish_remote_control_injection(event, true).await?;

    let call = handle
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events/delivery"))
        .await?;
    handle
        .ack_remote_control_injections(&cse_id, &[event], Utc::now(), call)
        .await?;

    let acked: bool =
        sqlx::query_scalar("SELECT acked_at IS NOT NULL FROM steering.interventions WHERE uuid=$1")
            .bind(intervention_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert!(acked, "the ack must stamp steering.interventions.acked_at");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn reserve_unclaimed_injection_for_lesson_source_has_no_owning_row() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = seed_session(&handle, &unique("/tmp/rc-lesson")).await?;
    let cse_id = crate::store::remote_control::support::seed_channel(&handle, session).await?;
    let event = Uuid::new_v4();

    assert!(
        handle
            .reserve_remote_control_injection(
                &cse_id,
                session,
                &PushSource::Lesson,
                event,
                b"frame"
            )
            .await?
            .is_some(),
        "a source with no owning row still inserts unconditionally once a channel exists"
    );
    let (source_kind, source_id): (String, Option<String>) = sqlx::query_as(
        "SELECT source_kind, source_id FROM remote_control.injections WHERE event_id=$1",
    )
    .bind(event)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(source_kind, "lesson");
    assert_eq!(source_id, None);

    handle.pool().close().await;
    Ok(())
}
