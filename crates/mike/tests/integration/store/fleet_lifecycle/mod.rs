use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
mod closure;
mod completion;
mod initialization;
mod launch_admission;
mod migration;
mod queries;
mod transitions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{FleetRow, NewFleet};
use mike::store::persistence::launch::NewRun;
use serde_json::json;
use uuid::Uuid;

use crate::test_db::{TestDb, TestDbHandle};

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

async fn starting(handle: &PersistHandle, run: Option<&str>) -> anyhow::Result<FleetRow> {
    let id = Uuid::new_v4();
    let path = format!("/tmp/fleet-lifecycle-{id}");
    if let Some(run_id) = run {
        handle
            .launch()
            .upsert_run(&NewRun {
                run_id: run_id.into(),
                project_path: Some(path.clone()),
                backend: "herdr".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
    }
    let row = handle
        .fleet()
        .insert_fleet(&NewFleet {
            slug: format!("lifecycle-{id}"),
            project_path: path.clone(),
            definition: json!({"repo": path}),
            missing: json!([]),
            created_by: None,
        })
        .await?;
    let ready = handle
        .fleet()
        .transition_fleet_state(&row, "ready", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("ready transition lost"))?;
    handle
        .fleet()
        .transition_fleet_state(&ready, "starting", run, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("starting transition lost"))
}

async fn running(handle: &PersistHandle, run: &str) -> anyhow::Result<FleetRow> {
    let row = starting(handle, Some(run)).await?;
    handle
        .fleet()
        .transition_fleet_state(&row, "running", Some(run), None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("running transition lost"))
}

async fn stopping(handle: &PersistHandle, row: &FleetRow) -> anyhow::Result<FleetRow> {
    handle
        .fleet()
        .transition_fleet_state(row, "stopping", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("stopping transition lost"))
}

async fn opened_launch(handle: &PersistHandle, run: &str) -> anyhow::Result<i64> {
    use mike::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch};
    let mut launch = NewLaunch::new("herdr", "/tmp/fleet-lifecycle");
    launch.run_id = Some(run.into());
    launch.session_uuid = Some(Uuid::new_v4());
    let id = handle.launch().insert_launch(&launch).await?;
    handle
        .launch()
        .record_launch_opened(id, &LaunchSurfaceRefs::default())
        .await?;
    Ok(id)
}
