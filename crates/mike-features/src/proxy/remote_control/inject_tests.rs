use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::injection::InjectionGate;
use crate::store::RequestStore;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::NewIntervention;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

use super::inject::{PushDeps, deliver_or_enqueue};
use super::streams::ReplyStreams;

pub const CSE: &str = "cse_inject_test";

pub async fn seed(handle: &PersistHandle) -> anyhow::Result<(Uuid, Uuid)> {
    let session = Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar(
        "WITH p AS (INSERT INTO projects.projects(project_path, display_name) VALUES ($1, $1) RETURNING id) \
         INSERT INTO sessions.sessions(session_uuid, project_id) SELECT $2, id FROM p RETURNING id",
    )
    .bind(format!("/tmp/rc-inject-test-{session}"))
    .bind(session)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ($1,$2)")
        .bind(CSE)
        .bind(session)
        .execute(handle.pool())
        .await?;
    let steerer_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE name = 'verifier'")
            .fetch_one(handle.pool())
            .await?;
    let intervention_uuid = Uuid::new_v4();
    handle
        .steerer()
        .insert_intervention(NewIntervention {
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
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            verdict_json: None,
            operator_turn_id: None,
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;
    Ok((session, intervention_uuid))
}

pub fn steerer_reminder(intervention_uuid: Uuid) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Steerer {
            kind: "verifier".to_string(),
            intervention_uuid: intervention_uuid.to_string(),
            advisory: false,
        },
        payload:
            "<system-reminder>Steering note from the operator's proxy: focus</system-reminder>"
                .to_string(),
    }
}

pub fn register_autoack_stream(streams: &ReplyStreams) -> super::streams::Registration {
    let (registration, mut rx) = streams.register(CSE.to_string());
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let _ = frame.result.send(true);
        }
    });
    registration
}

pub fn deps(persist: PersistHandle, streams: ReplyStreams, gate: Arc<InjectionGate>) -> PushDeps {
    PushDeps {
        persist: Some(persist),
        remote_control_streams: streams,
        gate,
        store: Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default()))),
    }
}

#[tokio::test]
async fn steerer_source_push_reaches_the_registered_stream() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let deps = deps(handle.clone(), streams, Arc::new(InjectionGate::default()));

    deliver_or_enqueue(&deps, session, steerer_reminder(intervention_uuid)).await;

    assert!(
        deps.store
            .write()
            .await
            .claim_reminders(&session.to_string())
            .is_empty(),
        "a successful push must not also enqueue the reminder"
    );
    let (source_kind, emitted): (String, bool) = sqlx::query_as(
        "SELECT source_kind, emitted_at IS NOT NULL FROM remote_control.injections i \
         JOIN remote_control.channels c ON c.id = i.channel_id WHERE c.cse_id = $1",
    )
    .bind(CSE)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(source_kind, "steerer");
    assert!(emitted);
    let intervention_pushed: bool = sqlx::query_scalar(
        "SELECT pushed_at IS NOT NULL FROM steering.interventions WHERE uuid=$1",
    )
    .bind(intervention_uuid)
    .fetch_one(handle.pool())
    .await?;
    assert!(intervention_pushed);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn second_steerer_push_within_wake_interval_falls_back_to_queue() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, first_uuid) = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let gate = Arc::new(InjectionGate::default());
    let deps = deps(handle.clone(), streams, gate.clone());

    deliver_or_enqueue(&deps, session, steerer_reminder(first_uuid)).await;

    let second_steerer_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE name = 'verifier'")
            .fetch_one(handle.pool())
            .await?;
    let second_session_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
    let second_uuid = Uuid::new_v4();
    handle
        .steerer()
        .insert_intervention(NewIntervention {
            steerer_id: second_steerer_id,
            enrolment_id: None,
            session_id: second_session_id,
            verdict_code: "fired",
            mode_code: "augment",
            authority_code: "redirect",
            trigger_request_id: None,
            confidence: Some(0.9),
            off_thread_topic: None,
            payload_text: Some("second nudge"),
            missing_evidence: None,
            model: Some("test-model"),
            anchor_text: None,
            intervention_uuid: second_uuid,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            verdict_json: None,
            operator_turn_id: None,
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;

    deliver_or_enqueue(&deps, session, steerer_reminder(second_uuid)).await;

    let queued = deps
        .store
        .write()
        .await
        .claim_reminders(&session.to_string());
    assert_eq!(
        queued.len(),
        1,
        "the second push, refused by the wake interval, must fall back to the queue"
    );
    let still_unpushed: bool =
        sqlx::query_scalar("SELECT pushed_at IS NULL FROM steering.interventions WHERE uuid=$1")
            .bind(second_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert!(
        still_unpushed,
        "the wake interval must refuse before any reservation is attempted"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mailbox_push_is_not_subject_to_the_steerer_wake_interval() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, intervention_uuid) = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let gate = Arc::new(InjectionGate::default());
    let deps = deps(handle.clone(), streams, gate.clone());

    let first = steerer_reminder(intervention_uuid);
    deliver_or_enqueue(&deps, session, first.clone()).await;
    assert!(
        !gate.admit(&session.to_string(), &first).may_wake(),
        "the steerer wake must have been recorded, so a second wake is throttled"
    );

    let message_uuid = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message_uuid, session, None, "hi")
        .await?;
    let mailbox_reminder = SystemReminder {
        source: ReminderSource::SessionMailbox {
            message_uuid,
            from_session_uuid: None,
        },
        payload: "<system-reminder>Cross-session message from x</system-reminder>".to_string(),
    };
    deliver_or_enqueue(&deps, session, mailbox_reminder).await;

    assert!(
        deps.store
            .write()
            .await
            .claim_reminders(&session.to_string())
            .is_empty(),
        "the mailbox push must have reached the stream, not the fallback queue"
    );
    let consumed: bool = sqlx::query_scalar(
        "SELECT claimed_at IS NOT NULL FROM mailbox.messages WHERE message_uuid=$1",
    )
    .bind(message_uuid)
    .fetch_one(handle.pool())
    .await?;
    assert!(
        consumed,
        "the mailbox row must have been claimed by the push"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mailbox_push_is_skipped_when_the_channel_listener_is_live() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, _intervention_uuid) = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let _registration = register_autoack_stream(&streams);
    let deps = deps(handle.clone(), streams, Arc::new(InjectionGate::default()));
    crate::mcp_channel::listeners().mark_live(session);

    let message_uuid = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message_uuid, session, None, "hi via channel")
        .await?;
    let mailbox_reminder = SystemReminder {
        source: ReminderSource::SessionMailbox {
            message_uuid,
            from_session_uuid: None,
        },
        payload: "<system-reminder>Cross-session message from x</system-reminder>".to_string(),
    };
    deliver_or_enqueue(&deps, session, mailbox_reminder).await;

    assert_eq!(
        deps.store
            .write()
            .await
            .claim_reminders(&session.to_string())
            .len(),
        1,
        "with a live channel listener, the RC push must be skipped and the reminder must \
         fall back to the in-memory queue instead"
    );
    let claimed: bool = sqlx::query_scalar(
        "SELECT claimed_at IS NOT NULL FROM mailbox.messages WHERE message_uuid=$1",
    )
    .bind(message_uuid)
    .fetch_one(handle.pool())
    .await?;
    assert!(
        !claimed,
        "the mailbox row must not be claimed by the RC push when the channel listener is live"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
