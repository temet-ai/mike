use crate::store::persistence::test_support::{seed_fleet_done, seed_fleet_starting};
use serde_json::json;
use uuid::Uuid;

use super::{fixture, new_fleet};
use crate::store::persistence::PersistError;
use crate::store::persistence::launch::NewRun;

#[tokio::test]
async fn state_stamps_follow_the_state_and_are_never_set_by_another() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let row = f
        .handle
        .insert_fleet(&new_fleet("stamps", "/tmp/fleet-a"))
        .await?;
    let id = row.fleet_uuid;

    let ready = f.handle.set_fleet_state(id, "ready", None, None).await?;
    assert_eq!(ready.state, "ready");
    assert!(ready.ready_at.is_some());
    assert_eq!(ready.started_at, None);
    assert_eq!(ready.finished_at, None);
    assert!(ready.updated_at >= row.updated_at);

    let starting = f.handle.set_fleet_state(id, "starting", None, None).await?;
    assert_eq!(starting.state, "starting");
    assert_eq!(starting.ready_at, ready.ready_at);
    assert_eq!(starting.started_at, None);

    let running = f.handle.set_fleet_state(id, "running", None, None).await?;
    assert!(running.started_at.is_some());
    assert_eq!(running.ready_at, ready.ready_at);
    assert_eq!(running.finished_at, None);

    let stopping = f.handle.set_fleet_state(id, "stopping", None, None).await?;
    assert_eq!(stopping.started_at, running.started_at);
    assert_eq!(stopping.finished_at, None);
    let done = seed_fleet_done(&f.handle, id).await?;
    assert_eq!(done.state, "done");
    assert!(done.finished_at.is_some());
    assert_eq!(done.started_at, running.started_at);

    let failed_row = f
        .handle
        .insert_fleet(&new_fleet("fails", "/tmp/fleet-a"))
        .await?;
    let failed_starting = seed_fleet_starting(&f.handle, failed_row.fleet_uuid, None).await?;
    let failed = f
        .handle
        .set_fleet_state(failed_row.fleet_uuid, "failed", None, None)
        .await?;
    assert!(failed.finished_at.is_some());
    assert_eq!(failed.started_at, None);
    assert!(failed_starting.ready_at.is_some());
    assert_eq!(failed.ready_at, failed_starting.ready_at);

    let terminal_reset = f.handle.set_fleet_state(id, "draft", None, None).await;
    assert!(matches!(
        terminal_reset,
        Err(PersistError::DataInvariant(_))
    ));
    assert_eq!(f.handle.find_fleet(&id.to_string()).await?, Some(done));
    let draft = f
        .handle
        .set_fleet_state(failed.fleet_uuid, "draft", None, None)
        .await?;
    assert_eq!(draft.state, "draft");
    assert_eq!(draft.finished_at, failed.finished_at);
    f.db.cleanup().await
}

#[tokio::test]
async fn run_id_and_start_report_are_kept_when_the_argument_is_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    f.handle
        .upsert_run(&NewRun {
            run_id: run_id.clone(),
            project_path: Some("/tmp/fleet-a".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let row = f
        .handle
        .insert_fleet(&new_fleet("keeps", "/tmp/fleet-a"))
        .await?;
    let report = json!([{"seat": "coordinator", "launched": true}]);
    f.handle
        .set_fleet_state(row.fleet_uuid, "ready", None, None)
        .await?;

    let starting = f
        .handle
        .set_fleet_state(row.fleet_uuid, "starting", Some(&run_id), Some(&report))
        .await?;
    assert_eq!(starting.run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(starting.start_report, Some(report.clone()));

    let running = f
        .handle
        .set_fleet_state(row.fleet_uuid, "running", None, None)
        .await?;
    assert_eq!(running.run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(running.start_report, Some(report));

    let replacement = json!([]);
    let replaced = f
        .handle
        .set_fleet_state(row.fleet_uuid, "running", None, Some(&replacement))
        .await?;
    assert_eq!(replaced.start_report, Some(replacement));
    assert_eq!(replaced.run_id.as_deref(), Some(run_id.as_str()));

    let dangling = f
        .handle
        .set_fleet_state(row.fleet_uuid, "running", Some("run-never-recorded"), None)
        .await;
    assert!(
        dangling.is_err(),
        "a fleet must not name a run that was never recorded"
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn unknown_fleet_is_a_data_invariant_and_unknown_state_is_refused() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let missing = f
        .handle
        .set_fleet_state(Uuid::new_v4(), "ready", None, None)
        .await;
    assert!(matches!(missing, Err(PersistError::DataInvariant(_))));

    let row = f
        .handle
        .insert_fleet(&new_fleet("bad-state", "/tmp/fleet-a"))
        .await?;
    let refused = f
        .handle
        .set_fleet_state(row.fleet_uuid, "paused", None, None)
        .await;
    assert!(matches!(refused, Err(PersistError::DataInvariant(_))));
    let refused_by_schema =
        sqlx::query("UPDATE launch.fleets SET state = 'paused' WHERE fleet_uuid = $1")
            .bind(row.fleet_uuid)
            .execute(f.handle.pool())
            .await;
    assert!(matches!(refused_by_schema, Err(sqlx::Error::Database(_))));
    let stored = f.handle.find_fleet(&row.fleet_uuid.to_string()).await?;
    assert_eq!(stored.map(|r| r.state), Some("draft".to_owned()));
    f.db.cleanup().await
}
