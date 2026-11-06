use chrono::{SubsecRound, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::remote_control::{
    PushSource, RemoteControlCall, RemoteControlSignals,
};
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn call_at<'a>(cse_id: &'a str, method: &'a str, path: &'a str) -> RemoteControlCall<'a> {
    RemoteControlCall {
        cse_id,
        method,
        path,
        http_status: Some(200),
        request_body: None,
        request_body_sha256: None,
        request_body_size: None,
        response_body: None,
        response_body_sha256: None,
        response_body_size: None,
        created_at: Utc::now(),
        completed_at: Some(Utc::now()),
        signals: RemoteControlSignals::default(),
        payloads: &[],
    }
}

async fn seed_steerer_intervention(handle: &PersistHandle) -> anyhow::Result<(Uuid, i64, Uuid)> {
    let session = Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar(
        "WITH p AS (INSERT INTO projects.projects(project_path, display_name) VALUES ($1, $1) RETURNING id) \
         INSERT INTO sessions.sessions(session_uuid, project_id) SELECT $2, id FROM p RETURNING id",
    )
    .bind(format!("/tmp/rc-steerer-{session}"))
    .bind(session)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ('cse_injection',$1)")
        .bind(session)
        .execute(handle.pool())
        .await?;
    let steerer_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE name = 'verifier'")
            .fetch_one(handle.pool())
            .await?;
    let intervention_uuid = Uuid::new_v4();
    handle
        .insert_intervention(crate::store::persistence::steering::NewIntervention {
            steerer_id,
            enrolment_id: None,
            session_id,
            verdict_code: "fired",
            mode_code: "augment",
            authority_code: "redirect",
            trigger_request_id: None,
            confidence: Some(0.9),
            off_thread_topic: None,
            payload_text: Some("nudge text"),
            missing_evidence: None,
            model: Some("test-model"),
            anchor_text: None,
            intervention_uuid,
            expires_at: Utc::now() + chrono::Duration::minutes(15),
            verdict_json: None,
            operator_turn_id: None,
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;
    Ok((session, session_id, intervention_uuid))
}

async fn seed(handle: &PersistHandle) -> anyhow::Result<(Uuid, Uuid)> {
    let session = Uuid::new_v4();
    sqlx::query("WITH p AS (INSERT INTO projects.projects(project_path, display_name) VALUES ('/tmp/rc-injection', 'rc-injection') RETURNING id) INSERT INTO sessions.sessions(session_uuid, project_id) SELECT $1, id FROM p")
        .bind(session)
        .execute(handle.pool())
        .await?;
    let message = Uuid::new_v4();
    handle
        .insert_message(message, session, None, "test envelope")
        .await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ('cse_injection',$1)")
        .bind(session).execute(handle.pool()).await?;
    Ok((session, message))
}

#[tokio::test]
async fn reservation_prevents_turn_drain_and_failed_emission_releases_it() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, message) = seed(&handle).await?;
    let event = Uuid::new_v4();
    let source = PushSource::Mailbox {
        message_uuid: message,
    };
    assert!(
        handle
            .reserve_remote_control_injection(
                "cse_injection",
                Uuid::new_v4(),
                &source,
                event,
                b"frame"
            )
            .await?
            .is_none()
    );
    assert!(
        handle
            .reserve_remote_control_injection("cse_injection", session, &source, event, b"frame")
            .await?
            .is_some()
    );
    assert!(
        !handle.claim_message(message).await?,
        "turn drain cannot claim a queued frame too"
    );
    assert!(
        handle
            .reserve_remote_control_injection(
                "cse_injection",
                session,
                &source,
                Uuid::new_v4(),
                b"frame"
            )
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
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn ack_before_completion_write_is_idempotent_and_stops_redelivery() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, message) = seed(&handle).await?;
    let event = Uuid::new_v4();
    let source = PushSource::Mailbox {
        message_uuid: message,
    };
    assert!(
        handle
            .reserve_remote_control_injection("cse_injection", session, &source, event, b"frame")
            .await?
            .is_some()
    );
    let call = handle
        .record_remote_control_call(call_at("cse_injection", "POST", "/worker/events/delivery"))
        .await?;
    let received = Utc::now();
    handle
        .ack_remote_control_injections("cse_injection", &[event], received, call)
        .await?;
    handle.finish_remote_control_injection(event, true).await?;
    handle
        .ack_remote_control_injections(
            "cse_injection",
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
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reserve_steerer_injection_stamps_pushed_at_and_blocks_double_push() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, _session_id, intervention_uuid) = seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();

    let reserved_at = handle
        .reserve_remote_control_injection("cse_injection", session, &source, event, b"frame")
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
            .reserve_remote_control_injection(
                "cse_injection",
                session,
                &source,
                Uuid::new_v4(),
                b"frame"
            )
            .await?
            .is_none()
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn finish_steerer_injection_failed_emission_clears_pushed_at() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, _session_id, intervention_uuid) = seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    assert!(
        handle
            .reserve_remote_control_injection("cse_injection", session, &source, event, b"frame")
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
            .reserve_remote_control_injection(
                "cse_injection",
                session,
                &source,
                Uuid::new_v4(),
                b"frame"
            )
            .await?
            .is_some()
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn ack_steering_interventions_stamps_acked_at() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, _session_id, intervention_uuid) = seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    assert!(
        handle
            .reserve_remote_control_injection("cse_injection", session, &source, event, b"frame")
            .await?
            .is_some()
    );
    handle.finish_remote_control_injection(event, true).await?;

    let call = handle
        .record_remote_control_call(call_at("cse_injection", "POST", "/worker/events/delivery"))
        .await?;
    handle
        .ack_remote_control_injections("cse_injection", &[event], Utc::now(), call)
        .await?;

    let acked: bool =
        sqlx::query_scalar("SELECT acked_at IS NOT NULL FROM steering.interventions WHERE uuid=$1")
            .bind(intervention_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert!(acked, "the ack must stamp steering.interventions.acked_at");

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reserve_unclaimed_injection_for_lesson_source_has_no_owning_row() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    sqlx::query("WITH p AS (INSERT INTO projects.projects(project_path, display_name) VALUES ('/tmp/rc-lesson', 'rc-lesson') RETURNING id) INSERT INTO sessions.sessions(session_uuid, project_id) SELECT $1, id FROM p")
        .bind(session)
        .execute(handle.pool())
        .await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ('cse_injection',$1)")
        .bind(session)
        .execute(handle.pool())
        .await?;
    let event = Uuid::new_v4();

    assert!(
        handle
            .reserve_remote_control_injection(
                "cse_injection",
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
    db.cleanup().await?;
    Ok(())
}
