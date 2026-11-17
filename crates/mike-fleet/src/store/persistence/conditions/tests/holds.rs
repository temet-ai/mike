use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{fixture, new_condition, seeded_session, time_condition};
use crate::store::persistence::conditions::{ConditionRow, NewCondition};
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::test_support::seed_session_turns;
use crate::store::persistence::{PersistHandle, fleet::tests::seed::seed_run};

async fn holds(handle: &PersistHandle, new: &NewCondition) -> anyhow::Result<bool> {
    let row: ConditionRow = handle.fleet().insert_condition(new).await?;
    Ok(handle.fleet().condition_holds(&row, Utc::now()).await?)
}

#[tokio::test]
async fn time_condition_holds_once_not_before_has_passed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(holds(&f.handle, &time_condition()).await?);
    let mut later = time_condition();
    later.not_before = Utc::now() + Duration::hours(1);
    assert!(!holds(&f.handle, &later).await?);
    f.db.cleanup().await
}

#[tokio::test]
async fn session_turn_ended_holds_only_for_a_turn_closed_since_not_before() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = seeded_session(&f.handle).await?;
    seed_session_turns(
        f.handle.pool(),
        &session.uuid.to_string(),
        session.project_id,
        1,
    )
    .await?;
    let spec = json!({"session_uuid": session.uuid});

    let mut after_turn = new_condition("session_turn_ended", spec.clone());
    after_turn.not_before = Utc::now() - Duration::hours(1);
    assert!(holds(&f.handle, &after_turn).await?);

    let mut before_turn = new_condition("session_turn_ended", spec);
    before_turn.not_before = Utc::now() + Duration::hours(1);
    assert!(!holds(&f.handle, &before_turn).await?);

    let stranger = new_condition(
        "session_turn_ended",
        json!({"session_uuid": Uuid::new_v4()}),
    );
    assert!(!holds(&f.handle, &stranger).await?);
    f.db.cleanup().await
}

async fn register(
    handle: &PersistHandle,
    row_id: i64,
    project_id: i64,
    ended: bool,
) -> anyhow::Result<()> {
    let released_at = ended.then(|| Utc::now() - Duration::minutes(1));
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at, released_at)
         VALUES ($1, $2, '/tmp/conditions', 'mcp', now() + interval '15 minutes', $3)",
    )
    .bind(row_id)
    .bind(project_id)
    .bind(released_at)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn session_ended_holds_for_a_released_registration_not_a_live_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let ended = seeded_session(&f.handle).await?;
    register(&f.handle, ended.row_id, ended.project_id, true).await?;
    let live = seeded_session(&f.handle).await?;
    register(&f.handle, live.row_id, live.project_id, false).await?;

    let ended_condition = new_condition("session_ended", json!({"session_uuid": ended.uuid}));
    assert!(holds(&f.handle, &ended_condition).await?);
    let live_condition = new_condition("session_ended", json!({"session_uuid": live.uuid}));
    assert!(!holds(&f.handle, &live_condition).await?);
    f.db.cleanup().await
}

#[tokio::test]
async fn session_ended_holds_once_the_lease_has_lapsed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let lapsed = seeded_session(&f.handle).await?;
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at)
         VALUES ($1, $2, '/tmp/conditions', 'mcp', now() - interval '1 minute')",
    )
    .bind(lapsed.row_id)
    .bind(lapsed.project_id)
    .execute(f.handle.pool())
    .await?;
    let condition = new_condition("session_ended", json!({"session_uuid": lapsed.uuid}));
    assert!(holds(&f.handle, &condition).await?);
    f.db.cleanup().await
}

async fn record_event(
    handle: &PersistHandle,
    run: &str,
    task: &str,
    kind: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO launch.fleet_events (run_id, task, kind) VALUES ($1, $2, $3)")
        .bind(run)
        .bind(task)
        .bind(kind)
        .execute(handle.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn fleet_event_holds_for_a_matching_run_kind_and_task_only() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("conditions-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    record_event(&f.handle, &run, "T1", "check_finished").await?;

    let matching = new_condition(
        "fleet_event",
        json!({"run_id": run, "kind": "check_finished", "task": "T1"}),
    );
    assert!(holds(&f.handle, &matching).await?);
    let any_task = new_condition(
        "fleet_event",
        json!({"run_id": run, "kind": "check_finished"}),
    );
    assert!(holds(&f.handle, &any_task).await?);
    let other_kind = new_condition(
        "fleet_event",
        json!({"run_id": run, "kind": "commit_landed"}),
    );
    assert!(!holds(&f.handle, &other_kind).await?);
    let other_task = new_condition(
        "fleet_event",
        json!({"run_id": run, "kind": "check_finished", "task": "T2"}),
    );
    assert!(!holds(&f.handle, &other_task).await?);
    let mut future_since = new_condition(
        "fleet_event",
        json!({"run_id": run, "kind": "check_finished"}),
    );
    future_since.not_before = Utc::now() + Duration::hours(1);
    assert!(!holds(&f.handle, &future_since).await?);
    f.db.cleanup().await
}
