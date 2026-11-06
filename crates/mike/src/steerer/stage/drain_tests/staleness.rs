use crate::compress::manifest::ManifestBuilder;
use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::steerer::stage::drain;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};

use super::support::{
    drain_fixture, reverted_row, seed_intervention, seed_request_row, stamp_pushed_at,
    steerer_reminder, user_turn_body,
};

#[tokio::test]
async fn drain_expired_ttl_steerer_reminder_dropped_and_stamped() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, _project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_expired_ttl_steerer_reminder_dropped_and_stamped"
        );
        return Ok(());
    };

    let past = chrono::Utc::now() - chrono::Duration::minutes(1);
    let intervention_uuid = seed_intervention(&handle, &pool, session_id, trigger_id, past).await?;
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session_uuid, steerer_reminder(intervention_uuid));

    let mut body = user_turn_body();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        &state,
        &session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;

    assert!(
        injected.is_empty(),
        "expired-TTL reminder must not be injected"
    );
    assert!(
        state
            .store
            .write()
            .await
            .claim_reminders(&session_uuid)
            .is_empty(),
        "expired reminder must not be requeued either"
    );
    let (reverted, reason) = reverted_row(&pool, intervention_uuid).await?;
    assert!(reverted, "intervention row must be stamped reverted");
    assert_eq!(reason.as_deref(), Some("expired_ttl"));

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drain_superseded_steerer_reminder_dropped_and_stamped() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_superseded_steerer_reminder_dropped_and_stamped"
        );
        return Ok(());
    };

    seed_request_row(&pool, project_id, session_id, "a brand new ask").await?;

    let future = chrono::Utc::now() + chrono::Duration::minutes(10);
    let intervention_uuid =
        seed_intervention(&handle, &pool, session_id, trigger_id, future).await?;
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session_uuid, steerer_reminder(intervention_uuid));

    let mut body = user_turn_body();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        &state,
        &session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;

    assert!(
        injected.is_empty(),
        "superseded reminder must not be injected"
    );
    let (reverted, reason) = reverted_row(&pool, intervention_uuid).await?;
    assert!(reverted, "intervention row must be stamped reverted");
    assert_eq!(reason.as_deref(), Some("superseded_by_operator"));

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drain_mailbox_reminder_unaffected_by_steerer_expiry_logic() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, _project_id, _session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_mailbox_reminder_unaffected_by_steerer_expiry_logic"
        );
        return Ok(());
    };

    let recipient_uuid: uuid::Uuid = session_uuid.parse()?;
    let message_uuid = uuid::Uuid::new_v4();
    let inserted = handle
        .insert_message(message_uuid, recipient_uuid, None, "hello from a peer")
        .await?;
    assert!(inserted.is_some(), "mailbox insert must resolve recipient");

    let past = chrono::Utc::now() - chrono::Duration::minutes(1);
    let intervention_uuid =
        seed_intervention(&handle, &pool, _session_id, trigger_id, past).await?;

    {
        let mut store = state.store.write().await;
        store.enqueue_reminder(&session_uuid, steerer_reminder(intervention_uuid));
        store.enqueue_reminder(
            &session_uuid,
            SystemReminder {
                source: ReminderSource::SessionMailbox {
                    message_uuid,
                    from_session_uuid: None,
                },
                payload: "<system-reminder>mailbox</system-reminder>".to_string(),
            },
        );
    }

    let mut body = user_turn_body();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        &state,
        &session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;

    assert_eq!(
        injected.len(),
        1,
        "the mailbox reminder must still deliver even though the co-drained \
         steerer reminder expired"
    );
    assert!(matches!(
        injected[0].source,
        ReminderSource::SessionMailbox { .. }
    ));

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drain_steerer_reminder_pushed_within_grace_window_is_deferred() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, _project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_steerer_reminder_pushed_within_grace_window_is_deferred"
        );
        return Ok(());
    };

    let future = chrono::Utc::now() + chrono::Duration::minutes(10);
    let intervention_uuid =
        seed_intervention(&handle, &pool, session_id, trigger_id, future).await?;
    stamp_pushed_at(&pool, intervention_uuid, chrono::Utc::now()).await?;
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session_uuid, steerer_reminder(intervention_uuid));

    let mut body = user_turn_body();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        &state,
        &session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;

    assert!(
        injected.is_empty(),
        "a push within its grace window must not also be delivered here"
    );
    let requeued = state.store.write().await.claim_reminders(&session_uuid);
    assert_eq!(
        requeued.len(),
        1,
        "a deferred (not stale) reminder must be requeued for a later drain, not dropped"
    );
    let (reverted, _reason) = reverted_row(&pool, intervention_uuid).await?;
    assert!(
        !reverted,
        "a push-in-flight deferral must not stamp the intervention reverted"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drain_steerer_reminder_pushed_past_grace_window_falls_through_to_delivery()
-> anyhow::Result<()> {
    let Some((db, handle, pool, state, _project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_steerer_reminder_pushed_past_grace_window_falls_through_to_delivery"
        );
        return Ok(());
    };

    let future = chrono::Utc::now() + chrono::Duration::minutes(10);
    let intervention_uuid =
        seed_intervention(&handle, &pool, session_id, trigger_id, future).await?;
    let stale_push = chrono::Utc::now() - chrono::Duration::seconds(30);
    stamp_pushed_at(&pool, intervention_uuid, stale_push).await?;
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session_uuid, steerer_reminder(intervention_uuid));

    let mut body = user_turn_body();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        &state,
        &session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;

    assert_eq!(
        injected.len(),
        1,
        "an unacked push past its grace window must fall through to request-time delivery"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
