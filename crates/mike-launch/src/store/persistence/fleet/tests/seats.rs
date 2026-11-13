use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::fixture;
use super::seed::{micros_now, seed_coordinator_traffic, seed_launch, seed_run, seed_session};
use crate::store::persistence::session_goals::SetSessionIdentity;
use crate::store::persistence::test_support::seed_project;

#[tokio::test]
async fn seat_rows_list_opened_launches_oldest_first_with_role_and_context() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = seed_project(pool, "/tmp/fleet-a").await?;
    let base = micros_now()?;
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run_id).await?;

    let coordinator = Uuid::new_v4();
    let worker = Uuid::new_v4();
    let coordinator_sid = seed_session(pool, project_id, coordinator).await?;
    seed_session(pool, project_id, worker).await?;
    f.handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            session_uuid: coordinator,
            display_name: None,
            role: Some("coordinator"),
            goal: Some("drive the fleet"),
            next_steps: None,
            instructions: None,
            set_by: "test",
        })
        .await?;

    seed_launch(&f.handle, &run_id, "coordinator", Some(coordinator), true).await?;
    seed_launch(&f.handle, &run_id, "worker", Some(worker), true).await?;
    seed_launch(
        &f.handle,
        &run_id,
        "unregistered",
        Some(Uuid::new_v4()),
        true,
    )
    .await?;
    seed_launch(
        &f.handle,
        &run_id,
        "never-opened",
        Some(Uuid::new_v4()),
        false,
    )
    .await?;
    seed_run(&f.handle, "some-other-run").await?;
    seed_launch(&f.handle, "some-other-run", "elsewhere", None, true).await?;

    let last_at = seed_coordinator_traffic(pool, coordinator_sid, project_id, base).await?;

    let rows = f.handle.fleet().fleet_seat_rows(&run_id).await?;
    let seats: Vec<_> = rows.iter().map(|r| r.seat.as_deref()).collect();
    assert_eq!(
        seats,
        vec![Some("coordinator"), Some("worker"), Some("unregistered")]
    );

    let lead = &rows[0];
    assert_eq!(lead.role.as_deref(), Some("coordinator"));
    assert_eq!(lead.agent.as_deref(), Some("coordinator-agent"));
    assert_eq!(lead.model.as_deref(), Some("opus"));
    assert_eq!(lead.session_uuid, Some(coordinator));
    assert_eq!(lead.surface_ref.as_deref(), Some("pane-coordinator"));
    assert!(lead.opened_at.is_some());
    assert_eq!(lead.closed_at, None);
    assert_eq!(lead.requests, 3);
    assert_eq!(lead.last_request_at, Some(last_at));
    assert_eq!(lead.context_tokens, Some(175));

    let idle = &rows[1];
    assert_eq!(idle.role, None);
    assert_eq!(idle.requests, 0);
    assert_eq!(idle.last_request_at, None);
    assert_eq!(idle.context_tokens, None);

    let stranger = &rows[2];
    assert_eq!(stranger.role, None);
    assert_eq!(stranger.requests, 0);
    assert_eq!(stranger.context_tokens, None);

    assert!(
        f.handle
            .fleet()
            .fleet_seat_rows("run-with-no-launches")
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn seat_rows_carry_the_close_time_of_a_closed_launch() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run_id).await?;
    let launch = seed_launch(&f.handle, &run_id, "observer", None, true).await?;
    f.handle
        .launch()
        .record_launch_closed(launch, "seat retired", Some(true))
        .await?;

    let rows = f.handle.fleet().fleet_seat_rows(&run_id).await?;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].closed_at.is_some());
    assert_eq!(rows[0].session_uuid, None);
    f.db.cleanup().await
}
