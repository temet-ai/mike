use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::steerer::stage::drain;
use crate::store::models::RequestId;
use crate::wire::manifest::ManifestBuilder;

use super::support::{
    drain_fixture, reverted_row, seed_intervention, seed_request_row, seed_request_row_with_origin,
    steerer_reminder, user_turn_body,
};

#[tokio::test]
async fn drain_channel_wake_turn_does_not_supersede_still_delivers() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_channel_wake_turn_does_not_supersede_still_delivers"
        );
        return Ok(());
    };

    seed_request_row(
        &pool,
        project_id,
        session_id,
        "<channel source=\"wake\" reason=\"steering\" pending=\"1\">check in</channel>",
    )
    .await?;

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

    assert_eq!(
        injected.len(),
        1,
        "channel-wake turn must not supersede the payload it exists to deliver"
    );
    let (reverted, _reason) = reverted_row(&pool, intervention_uuid).await?;
    assert!(!reverted, "delivered intervention must not be reverted");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drain_monitor_sidecar_turn_does_not_supersede_still_delivers() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_monitor_sidecar_turn_does_not_supersede_still_delivers"
        );
        return Ok(());
    };

    seed_request_row_with_origin(
        &pool,
        project_id,
        session_id,
        "Current state: done (for 0m)\nTool calls so far: none",
        "monitor_sidecar",
    )
    .await?;

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

    assert_eq!(
        injected.len(),
        1,
        "monitor_sidecar recap must not supersede — it isn't an operator turn"
    );
    let (reverted, _reason) = reverted_row(&pool, intervention_uuid).await?;
    assert!(!reverted, "delivered intervention must not be reverted");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn drain_fresh_mid_turn_steerer_reminder_still_delivers() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             drain_fresh_mid_turn_steerer_reminder_still_delivers"
        );
        return Ok(());
    };

    seed_request_row(&pool, project_id, session_id, "").await?;

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

    assert_eq!(injected.len(), 1, "fresh mid-turn reminder must deliver");
    let (reverted, _reason) = reverted_row(&pool, intervention_uuid).await?;
    assert!(!reverted, "delivered intervention must not be reverted");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
