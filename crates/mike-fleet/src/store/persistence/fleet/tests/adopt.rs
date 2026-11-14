use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::Utc;
use uuid::Uuid;

use super::fixture;
use super::seed::{seed_launch, seed_request, seed_response, seed_run, seed_session, usage};
use crate::launch::fleet::definition::{FleetDefinition, TaskDefinition, WorkerDefinition};
use crate::launch::fleet::staff::unstarted_tasks;
use crate::launch::fleet::status::{self, FleetHeader, SeatFacts};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::seed_project;

fn definition() -> FleetDefinition {
    let mut def = FleetDefinition::default();
    def.workers
        .insert("db-dev".to_string(), WorkerDefinition::default());
    def.tasks.insert(
        "T1".to_string(),
        TaskDefinition {
            discipline: "db-dev".to_string(),
            ..TaskDefinition::default()
        },
    );
    def
}

async fn status_of(
    handle: &PersistHandle,
    run: &str,
) -> anyhow::Result<crate::launch::fleet::status::FleetStatus> {
    let seats: Vec<SeatFacts> = handle
        .fleet()
        .fleet_seat_rows(run)
        .await?
        .into_iter()
        .map(|row| SeatFacts {
            seat: row.seat,
            session: row.session_uuid,
            role: row.role,
            agent: row.agent,
            model: row.model,
            opened_at: row.opened_at,
            closed_at: row.closed_at,
            requests: row.requests,
            last_request_at: row.last_request_at,
            context_tokens: row.context_tokens,
            live_state: None,
            last_status: row.last_status,
            first_request_cache: None,
        })
        .collect();
    let header = FleetHeader {
        fleet: Uuid::new_v4().to_string(),
        slug: "adopt".to_string(),
        state: "running".to_string(),
        run_id: Some(run.to_string()),
        started_at: None,
        finished_at: None,
        now: Utc::now(),
        missing: Vec::new(),
    };
    Ok(status::build(header, &definition(), &seats, &[]))
}

#[tokio::test]
async fn a_launch_never_marked_opened_whose_session_sends_requests_is_adopted_not_relaunched()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = seed_project(pool, "/tmp/fleet-adopt").await?;
    let run = format!("fleet-adopt-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let session = Uuid::new_v4();
    let session_id = seed_session(pool, project_id, session).await?;
    let launch = seed_launch(&f.handle, &run, "db-dev-T1", Some(session), false).await?;

    assert!(
        f.handle
            .launch()
            .adopt_live_fleet_launches(&run)
            .await?
            .is_empty()
    );
    let before = status_of(&f.handle, &run).await?;
    assert!(unstarted_tasks(&before).contains("T1"));

    let request = seed_request(pool, session_id, project_id, None, Utc::now()).await?;
    seed_response(pool, request, &usage(10, 0, 0)).await?;
    let adopted = f.handle.launch().adopt_live_fleet_launches(&run).await?;
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].id, launch);
    assert_eq!(adopted[0].seat.as_deref(), Some("db-dev-T1"));

    let after = status_of(&f.handle, &run).await?;
    assert!(!unstarted_tasks(&after).contains("T1"), "{:?}", after.tasks);
    assert_eq!(after.tasks[0].state, "working");
    assert!(
        f.handle
            .launch()
            .adopt_live_fleet_launches(&run)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn adoption_skips_launches_without_requests_closed_launches_and_other_runs()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = seed_project(pool, "/tmp/fleet-adopt").await?;
    let run = format!("fleet-adopt-{}", Uuid::new_v4());
    let other = format!("fleet-other-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    seed_run(&f.handle, &other).await?;
    let (quiet, closed, foreign) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    seed_launch(&f.handle, &run, "db-dev-T1", Some(quiet), false).await?;
    let closed_launch = seed_launch(&f.handle, &run, "db-dev-T2", Some(closed), false).await?;
    seed_launch(&f.handle, &other, "db-dev-T3", Some(foreign), false).await?;
    for session in [closed, foreign] {
        let id = seed_session(pool, project_id, session).await?;
        let request = seed_request(pool, id, project_id, None, Utc::now()).await?;
        seed_response(pool, request, &usage(10, 0, 0)).await?;
    }
    f.handle
        .launch()
        .record_launch_closed(closed_launch, "retired", Some(true))
        .await?;

    assert!(
        f.handle
            .launch()
            .adopt_live_fleet_launches(&run)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn closing_an_unopened_fleet_launch_works_and_settles_it() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-close-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let launch = seed_launch(&f.handle, &run, "db-dev-T1", Some(Uuid::new_v4()), false).await?;
    assert_eq!(
        f.handle.fleet().unsettled_fleet_launches(&run).await?.len(),
        1
    );

    assert!(
        f.handle
            .launch()
            .record_fleet_launch_closure(launch, "fleet-elsewhere", "pane closed", Some(true))
            .await
            .is_err()
    );
    f.handle
        .launch()
        .record_launch_closed(launch, "pane closed", Some(true))
        .await?;
    f.handle
        .launch()
        .record_launch_closed(launch, "pane closed again", Some(true))
        .await?;

    assert!(
        f.handle
            .fleet()
            .unsettled_fleet_launches(&run)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}
