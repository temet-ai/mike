use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::proxy_repo::ProxyPersist as _;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use chrono::Duration;
use uuid::Uuid;

use super::tests_observation_support::{fixture, observation};
use super::{ObservationApplyOutcome, ObservationLimits};
use crate::runtime::{ModelCallObservation, SessionLifecycle, UsageSnapshot};
use crate::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};
use crate::store::persistence::rehydrate::fetch_session_summary;
use crate::workspace::{Liveness, RosterScope};

#[tokio::test]
async fn observation_third_runtime_drives_roster_fleet_and_summary() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut o = observation(f.session)?;
    o.model_call = Some(ModelCallObservation {
        call_id: "call-one".into(),
        status: Some("completed".into()),
        duration_ms: Some(42),
        http_status: Some(200),
        usage: Some(UsageSnapshot {
            scope: "own".into(),
            basis: "call".into(),
            input_tokens: Some(100),
            cached_input_tokens: Some(40),
            output_tokens: Some(7),
            ..Default::default()
        }),
    });
    f.handle
        .launch()
        .upsert_run(&NewRun {
            run_id: "third-fixture".into(),
            project_path: None,
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/third-runtime");
    launch.run_id = Some("third-fixture".into());
    launch.seat = Some("worker".into());
    launch.session_uuid = Some(f.session);
    let launch_id = f.handle.launch().insert_launch(&launch).await?;
    f.handle
        .launch()
        .record_launch_opened(launch_id, &LaunchSurfaceRefs::default())
        .await?;
    assert_eq!(
        f.handle.runtime().apply_session_observation(&o).await?,
        ObservationApplyOutcome::Applied
    );
    let roster = f
        .handle
        .proxy()
        .fetch_workspace_roster(f.project, RosterScope::Live)
        .await?;
    assert_eq!(roster[0].liveness, Liveness::Live);
    assert_eq!(roster[0].last_request_at, Some(o.observed_at));
    let fleet = f.handle.fleet().fleet_seat_rows("third-fixture").await?;
    assert_eq!(fleet[0].model.as_deref(), Some("actual-model"));
    assert_eq!(fleet[0].requests, 1);
    assert_eq!(fleet[0].context_tokens, Some(100));
    let summary = fetch_session_summary(f.handle.pool(), f.session_db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session summary missing"))?;
    assert_eq!(summary.request_count, 1);
    assert_eq!(summary.last_models, vec!["actual-model"]);
    let facts = f
        .handle
        .runtime()
        .session_observations(&[f.session])
        .await?;
    assert_eq!(facts[0].runtime, "third-fixture");
    assert_eq!(facts[0].input_tokens, Some(100));
    assert_eq!(facts[0].usage_basis.as_deref(), Some("observed_calls"));
    f.cleanup().await
}

#[tokio::test]
async fn observation_replay_atomic_failure_and_provider_order_preserve_facts() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut o = observation(f.session)?;
    o.model_call = Some(ModelCallObservation {
        call_id: "call-one".into(),
        status: None,
        duration_ms: Some(-1),
        http_status: None,
        usage: None,
    });
    assert!(
        f.handle
            .runtime()
            .apply_session_observation(&o)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .runtime()
            .session_observation_time(f.session, o.observation_id)
            .await?
            .is_none()
    );
    o.model_call
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("missing call"))?
        .duration_ms = Some(5);
    assert_eq!(
        f.handle.runtime().apply_session_observation(&o).await?,
        ObservationApplyOutcome::Applied
    );
    assert_eq!(
        f.handle.runtime().apply_session_observation(&o).await?,
        ObservationApplyOutcome::Replayed
    );
    let mut conflict = o.clone();
    conflict.observed_model = Some("conflicting-model".into());
    assert!(
        f.handle
            .runtime()
            .apply_session_observation(&conflict)
            .await
            .is_err()
    );
    let mut wire = o.clone();
    wire.observation_id = Uuid::new_v4();
    wire.observed_at += Duration::seconds(10);
    wire.lifecycle = None;
    wire.control = None;
    wire.usage = None;
    f.handle.runtime().apply_session_observation(&wire).await?;
    o.observation_id = Uuid::new_v4();
    o.observed_at += Duration::seconds(1);
    o.lifecycle = Some(SessionLifecycle::Ended);
    o.usage = Some(UsageSnapshot {
        scope: "thread".into(),
        basis: "provider_cumulative".into(),
        input_tokens: Some(50),
        ..Default::default()
    });
    f.handle.runtime().apply_session_observation(&o).await?;
    let mut stale = o.clone();
    stale.observation_id = Uuid::new_v4();
    stale.observed_at -= Duration::seconds(2);
    stale.lifecycle = Some(SessionLifecycle::Active);
    f.handle.runtime().apply_session_observation(&stale).await?;
    let facts = f
        .handle
        .runtime()
        .session_observations(&[f.session])
        .await?;
    assert_eq!(facts[0].lifecycle.as_deref(), Some("ended"));
    assert_eq!(facts[0].input_tokens, Some(50));
    assert_eq!(facts[0].output_tokens, None);
    assert_eq!(facts[0].model_call_count, 1);
    assert_eq!(
        f.handle.proxy().fetch_session_liveness(f.session).await?,
        Some(Liveness::Ended)
    );
    o.observation_id = Uuid::new_v4();
    o.observed_at += Duration::seconds(2);
    o.lifecycle = Some(SessionLifecycle::Active);
    f.handle.runtime().apply_session_observation(&o).await?;
    assert_eq!(
        f.handle.proxy().fetch_session_liveness(f.session).await?,
        Some(Liveness::Live)
    );
    f.cleanup().await
}

#[tokio::test]
async fn observation_concurrent_quota_refusal_rolls_back_receipt_and_refunds_cascade()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = observation(f.session)?;
    let mut second = first.clone();
    second.observation_id = Uuid::new_v4();
    let limits = ObservationLimits {
        max_rows: 2,
        ..Default::default()
    };
    let (a, b) = tokio::join!(
        f.handle
            .runtime()
            .apply_session_observation_with_limits(&first, &limits),
        f.handle
            .runtime()
            .apply_session_observation_with_limits(&second, &limits)
    );
    let outcomes = [a?, b?];
    assert!(outcomes.contains(&ObservationApplyOutcome::Applied));
    assert!(outcomes.contains(&ObservationApplyOutcome::BudgetRefused));
    let used = sqlx::query!(
        "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts'"
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(used.retained_rows, 2);
    assert!(used.retained_bytes > 128);
    sqlx::query!("DELETE FROM sessions.sessions WHERE id = $1", f.session_db)
        .execute(f.handle.pool())
        .await?;
    let used = sqlx::query!(
        "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts'"
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!((used.retained_rows, used.retained_bytes), (0, 0));
    f.cleanup().await
}
