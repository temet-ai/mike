use chrono::Utc;
use mike::store::persistence::proxy_repo::ProxyPersist as _;
use uuid::Uuid;

use mike::store::persistence::remote_control::PushSource;

use crate::store::remote_control::support::{call_at, seed_steerer_intervention, shared_handle};

#[tokio::test]
async fn release_stale_steerer_push_releases_matching_unacked_claim() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _session_id, intervention_uuid, cse_id) =
        seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    let reserved_at = handle
        .proxy()
        .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
        .await?
        .ok_or_else(|| anyhow::anyhow!("reservation must succeed"))?;
    handle
        .proxy()
        .finish_remote_control_injection(event, true)
        .await?;

    let released = handle
        .proxy()
        .release_stale_steerer_push(intervention_uuid, reserved_at)
        .await?;
    assert!(
        released,
        "a matching pushed_at on an unacked intervention must release"
    );

    let pushed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT pushed_at FROM steering.interventions WHERE uuid=$1")
            .bind(intervention_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert!(
        pushed_at.is_none(),
        "the release must clear pushed_at so the intervention becomes claimable again"
    );

    assert!(
        handle
            .proxy()
            .reserve_remote_control_injection(&cse_id, session, &source, Uuid::new_v4(), b"frame")
            .await?
            .is_some()
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn release_stale_steerer_push_noop_on_mismatched_pushed_at() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _session_id, intervention_uuid, cse_id) =
        seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    let reserved_at = handle
        .proxy()
        .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
        .await?
        .ok_or_else(|| anyhow::anyhow!("reservation must succeed"))?;
    handle
        .proxy()
        .finish_remote_control_injection(event, true)
        .await?;

    let stale_timer_value = reserved_at - chrono::Duration::seconds(1);
    let released = handle
        .proxy()
        .release_stale_steerer_push(intervention_uuid, stale_timer_value)
        .await?;
    assert!(
        !released,
        "a mismatched pushed_at must not release the current claim"
    );

    let pushed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT pushed_at FROM steering.interventions WHERE uuid=$1")
            .bind(intervention_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        pushed_at,
        Some(reserved_at),
        "the current claim's pushed_at must survive an unmatched release attempt"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn release_stale_steerer_push_noop_once_acked() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _session_id, intervention_uuid, cse_id) =
        seed_steerer_intervention(&handle).await?;
    let source = PushSource::Steerer { intervention_uuid };
    let event = Uuid::new_v4();
    let reserved_at = handle
        .proxy()
        .reserve_remote_control_injection(&cse_id, session, &source, event, b"frame")
        .await?
        .ok_or_else(|| anyhow::anyhow!("reservation must succeed"))?;
    handle
        .proxy()
        .finish_remote_control_injection(event, true)
        .await?;

    let call = handle
        .proxy()
        .record_remote_control_call(call_at(&cse_id, "POST", "/worker/events/delivery"))
        .await?;
    handle
        .proxy()
        .ack_remote_control_injections(&cse_id, &[event], Utc::now(), call)
        .await?;

    let released = handle
        .proxy()
        .release_stale_steerer_push(intervention_uuid, reserved_at)
        .await?;
    assert!(
        !released,
        "an already-acked intervention must not be released by a late fallback timer"
    );

    let acked: bool =
        sqlx::query_scalar("SELECT acked_at IS NOT NULL FROM steering.interventions WHERE uuid=$1")
            .bind(intervention_uuid)
            .fetch_one(handle.pool())
            .await?;
    assert!(acked, "acked_at must remain stamped");

    handle.pool().close().await;
    Ok(())
}
