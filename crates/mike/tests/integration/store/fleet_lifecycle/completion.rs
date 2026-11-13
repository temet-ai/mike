use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::json;
use uuid::Uuid;

use super::{fixture, opened_launch, running, starting, stopping};

#[tokio::test]
async fn completion_waits_for_verified_exits_and_terminal_command_receipts() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("completion-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let launch = opened_launch(&f.handle, &run).await?;
    let receipt = f
        .handle
        .fleet()
        .begin_tree_check(
            &format!("{run}#gate:isolated-source-v2"),
            "state",
            "test command",
            "worker",
        )
        .await?;
    let stopped = stopping(&f.handle, &row).await?;
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .launch()
            .find_run(&run)
            .await?
            .is_some_and(|run| run.closed_at.is_none())
    );
    f.handle
        .fleet()
        .record_fleet_launch_verified_exit(launch, &run, "observed exit")
        .await?;
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_none()
    );
    f.handle
        .fleet()
        .finish_tree_check(receipt, None, "cancelled after partial output", None)
        .await?;
    let done = f
        .handle
        .fleet()
        .complete_fleet_run(&stopped, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("settled fleet did not complete"))?;
    assert_eq!(done.state, "done");
    assert_eq!(done.started_at, row.started_at);
    assert!(done.finished_at.is_some());
    let closed = f
        .handle
        .launch()
        .find_run(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run missing"))?;
    assert!(closed.closed_at.is_some());
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_none()
    );
    assert_eq!(
        f.handle
            .fleet()
            .find_fleet(&done.fleet_uuid.to_string())
            .await?,
        Some(done.clone())
    );
    assert_eq!(
        f.handle
            .fleet()
            .set_fleet_state(done.fleet_uuid, "done", None, None)
            .await?,
        done
    );
    assert!(
        f.handle
            .fleet()
            .set_fleet_state(done.fleet_uuid, "stopping", None, None)
            .await
            .is_err()
    );
    assert_eq!(f.handle.launch().find_run(&run).await?, Some(closed));
    f.db.cleanup().await
}

#[tokio::test]
async fn stopping_before_start_associates_a_run_completes_without_inventing_one()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let start = starting(&f.handle, None).await?;
    let stopped = stopping(&f.handle, &start).await?;
    let done = f
        .handle
        .fleet()
        .complete_fleet_run(&stopped, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no-run stopping did not complete"))?;
    assert_eq!(done.state, "done");
    assert!(done.run_id.is_none());
    assert!(done.started_at.is_none());
    assert!(done.finished_at.is_some());
    assert!(
        f.handle
            .fleet()
            .transition_fleet_state(&start, "starting", Some("later-run"), None)
            .await?
            .is_none()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn retrospective_is_staged_before_done_and_stopping_retains_checkout() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("retro-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let mut definition = row.definition.clone();
    definition["retrospective"] = json!({"enabled": true});
    let saved_at = f
        .handle
        .fleet()
        .save_fleet_definition(row.fleet_uuid, &definition, &row.missing, row.updated_at)
        .await?
        .ok_or_else(|| anyhow::anyhow!("definition changed"))?;
    let enabled = row.saved(definition.clone(), row.missing.clone(), saved_at);
    let stopped = stopping(&f.handle, &enabled).await?;
    assert!(
        f.handle
            .fleet()
            .fleet_checkout_users()
            .await?
            .iter()
            .any(|row| row.fleet_uuid == stopped.fleet_uuid)
    );
    assert!(
        !f.handle
            .fleet()
            .pending_fleet_retrospectives()
            .await?
            .iter()
            .any(|row| row.fleet_uuid == stopped.fleet_uuid)
    );
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_none()
    );
    definition["retrospective_job"] = json!({"source_run": "other", "phase": "pending"});
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, Some(&definition))
            .await?
            .is_none()
    );
    definition["retrospective_job"] = json!({"source_run": run, "phase": "pending"});
    let done = f
        .handle
        .fleet()
        .complete_fleet_run(&stopped, Some(&definition))
        .await?
        .ok_or_else(|| anyhow::anyhow!("staged fleet did not complete"))?;
    assert_eq!(done.definition, definition);
    assert_eq!(
        f.handle
            .fleet()
            .pending_fleet_retrospectives()
            .await?
            .iter()
            .filter(|row| row.fleet_uuid == done.fleet_uuid)
            .count(),
        1
    );
    assert!(
        f.handle
            .fleet()
            .fleet_checkout_users()
            .await?
            .iter()
            .any(|row| row.fleet_uuid == done.fleet_uuid && row.state == "retrospective")
    );
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, Some(&definition))
            .await?
            .is_none()
    );
    assert_eq!(
        f.handle
            .fleet()
            .find_fleet(&done.fleet_uuid.to_string())
            .await?,
        Some(done)
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn stale_definition_or_lifecycle_cannot_close_the_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("stale-{}", Uuid::new_v4());
    let running = running(&f.handle, &run).await?;
    let stopped = stopping(&f.handle, &running).await?;
    let definition = json!({"repo": "/tmp/updated-after-stopping", "kept": true});
    f.handle
        .fleet()
        .save_fleet_definition(
            stopped.fleet_uuid,
            &definition,
            &stopped.missing,
            stopped.updated_at,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("definition changed"))?;
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, Some(&stopped.definition))
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&running, None)
            .await?
            .is_none()
    );
    let mut wrong_run = stopped.clone();
    wrong_run.run_id = Some("other".into());
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&wrong_run, None)
            .await?
            .is_none()
    );
    assert!(
        f.handle
            .launch()
            .find_run(&run)
            .await?
            .is_some_and(|run| run.closed_at.is_none())
    );
    let fresh = f
        .handle
        .fleet()
        .find_fleet(&stopped.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing"))?;
    assert_eq!(fresh.definition, definition);
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&fresh, None)
            .await?
            .is_some()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn fleet_write_failure_rolls_back_run_closure() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("rollback-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let stopped = stopping(&f.handle, &row).await?;
    sqlx::query(
        "CREATE FUNCTION launch.test_reject_done() RETURNS trigger LANGUAGE plpgsql AS
         $$ BEGIN IF NEW.state = 'done' THEN RAISE EXCEPTION 'test refuses done'; END IF;
            RETURN NEW; END $$",
    )
    .execute(f.handle.pool())
    .await?;
    sqlx::query(
        "CREATE TRIGGER test_reject_done BEFORE UPDATE ON launch.fleets
         FOR EACH ROW EXECUTE FUNCTION launch.test_reject_done()",
    )
    .execute(f.handle.pool())
    .await?;
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .launch()
            .find_run(&run)
            .await?
            .is_some_and(|run| run.closed_at.is_none())
    );
    assert_eq!(
        f.handle
            .fleet()
            .find_fleet(&row.fleet_uuid.to_string())
            .await?,
        Some(stopped.clone())
    );
    sqlx::query("DROP TRIGGER test_reject_done ON launch.fleets")
        .execute(f.handle.pool())
        .await?;
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_some()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn malformed_retrospective_flag_is_an_error_without_run_closure() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("bad-retro-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let stopped = stopping(&f.handle, &row).await?;
    let bad = json!({"retrospective": {"enabled": "maybe"}});
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, Some(&bad))
            .await
            .is_err()
    );
    assert!(
        f.handle
            .launch()
            .find_run(&run)
            .await?
            .is_some_and(|run| run.closed_at.is_none())
    );
    let mut missing = stopped.clone();
    missing.fleet_uuid = Uuid::new_v4();
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&missing, None)
            .await?
            .is_none()
    );
    f.db.cleanup().await
}
