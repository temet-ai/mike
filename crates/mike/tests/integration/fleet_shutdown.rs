use mike::config::ProxyConfig;
use mike::launch::fleet::shutdown;
use mike::launch::fleet::test_seams::durable;
use mike::state::AppState;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{FleetRow, NewFleet};
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

const FIXTURE_CHILD_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

struct Fixture {
    persist: PersistHandle,
    state: AppState,
    row: FleetRow,
    run: NewRun,
    repo: TempDir,
    db: TestDbHandle,
}

impl Fixture {
    async fn starting() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(None);
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let fleet_db = persist.fleet();
        let repo = tempfile::tempdir()?;
        let row = fleet_db
            .insert_fleet(&NewFleet {
                slug: format!("shutdown-{}", Uuid::new_v4()),
                project_path: repo.path().display().to_string(),
                definition: json!({"repo": repo.path(), "goal": "shutdown fixture"}),
                missing: json!([]),
                created_by: None,
            })
            .await?;
        let ready = fleet_db
            .transition_fleet_state(&row, "ready", None, None)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture did not become ready"))?;
        let row = fleet_db
            .transition_fleet_state(&ready, "starting", None, None)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture did not become starting"))?;
        let run = NewRun {
            run_id: format!("fleet-{}-12345678", row.slug),
            project_path: Some(repo.path().display().to_string()),
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        };
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(persist.clone());
        Ok(Some(Self {
            persist,
            state,
            row,
            run,
            repo,
            db,
        }))
    }

    async fn running(&mut self) -> anyhow::Result<()> {
        let initialized = self
            .persist
            .fleet()
            .initialize_fleet_run(&self.row, &self.run)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture run initialization failed"))?;
        self.row = self
            .persist
            .fleet()
            .transition_fleet_state(
                &initialized,
                "running",
                None,
                Some(&json!({"original": "start evidence"})),
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture did not become running"))?;
        Ok(())
    }

    async fn refresh(&self) -> anyhow::Result<FleetRow> {
        self.persist
            .fleet()
            .find_fleet(&self.row.fleet_uuid.to_string())
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture fleet disappeared"))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn stop_before_run_initialization_prevents_stale_start_reopening_run() -> anyhow::Result<()> {
    let Some(f) = Fixture::starting().await? else {
        return Ok(());
    };
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "done");
    assert!(
        f.persist
            .fleet()
            .initialize_fleet_run(&f.row, &f.run)
            .await?
            .is_none()
    );
    assert!(f.persist.launch().find_run(&f.run.run_id).await?.is_none());
    assert_eq!(f.refresh().await?.state, "done");
    f.cleanup().await
}

#[tokio::test]
async fn stop_unproven_closed_launch_retains_checkout_until_verified_exit() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let marker = f.repo.path().join("kept.txt");
    std::fs::write(&marker, "must remain while stopping")?;
    let mut launch = NewLaunch::new("none", f.repo.path().display().to_string());
    launch.run_id = Some(f.run.run_id.clone());
    launch.session_uuid = Some(Uuid::new_v4());
    launch.seat = Some("worker".into());
    let id = f.persist.launch().insert_launch(&launch).await?;
    f.persist
        .launch()
        .record_launch_opened(id, &LaunchSurfaceRefs::default())
        .await?;
    f.persist
        .launch()
        .record_launch_closed(id, "old unverified closure", Some(false))
        .await?;
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "stopping");
    assert_eq!(report.seats.len(), 1);
    assert_eq!(report.seats[0].process_exited, None);
    assert!(marker.is_file());
    assert!(f.refresh().await?.finished_at.is_none());
    f.persist
        .fleet()
        .record_fleet_launch_verified_exit(id, &f.run.run_id, "fixture exit proof")
        .await?;
    let stopped = f.refresh().await?;
    let report = shutdown::stop(&f.state, &f.persist, &stopped).await?;
    assert_eq!(report.state, "done");
    let done = f.refresh().await?;
    assert_eq!(done.start_report, f.row.start_report);
    assert!(done.finished_at.is_some());
    assert!(
        f.persist
            .launch()
            .find_run(&f.run.run_id)
            .await?
            .is_some_and(|run| run.closed_at.is_some())
    );
    let repeated = shutdown::stop(&f.state, &f.persist, &done).await?;
    assert_eq!(repeated.state, "done");
    assert_eq!(f.refresh().await?, done);
    f.cleanup().await
}

#[tokio::test]
async fn stop_orphaned_command_receipt_remains_stopping_without_fabricated_completion()
-> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let ledger = format!("{}#command:fixture", f.run.run_id);
    let receipt = f
        .persist
        .fleet()
        .begin_tree_check(&ledger, "state", "fake command", "worker")
        .await?;
    let before = f.persist.fleet().latest_tree_check(&ledger).await?;
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "stopping");
    assert_eq!(report.managed.pending_receipts, vec![receipt]);
    assert!(!report.managed.known_in_this_process);
    assert_eq!(f.persist.fleet().latest_tree_check(&ledger).await?, before);
    assert!(
        f.persist
            .launch()
            .find_run(&f.run.run_id)
            .await?
            .is_some_and(|run| run.closed_at.is_none())
    );
    f.persist
        .fleet()
        .finish_tree_check(
            receipt,
            None,
            "fixture cancellation: partial output",
            Some("state:cancelled"),
        )
        .await?;
    let report = shutdown::stop(&f.state, &f.persist, &f.refresh().await?).await?;
    assert_eq!(report.state, "done");
    assert!(
        f.persist
            .fleet()
            .latest_tree_check(&ledger)
            .await?
            .is_some_and(|row| !row.passed())
    );
    f.cleanup().await
}

#[tokio::test]
async fn stop_abandons_receipts_past_the_command_limit_and_finishes() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let ledger = format!("{}#command:fixture", f.run.run_id);
    let receipt = f
        .persist
        .fleet()
        .begin_tree_check(&ledger, "state", "fake command", "worker")
        .await?;
    sqlx::query(
        "UPDATE launch.tree_checks SET started_at = now() - interval '3 hours' WHERE id = $1",
    )
    .bind(receipt)
    .execute(f.persist.pool())
    .await?;
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "done");
    let row = f
        .persist
        .fleet()
        .latest_tree_check(&ledger)
        .await?
        .ok_or_else(|| anyhow::anyhow!("receipt gone"))?;
    assert!(!row.passed());
    assert_eq!(row.state_after.as_deref(), Some("abandoned:unverified"));
    assert!(row.output.contains("Abandoned at fleet stop"));
    f.cleanup().await
}

#[tokio::test]
async fn stop_stages_retrospective_before_atomic_done_without_rewriting_start_evidence()
-> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let mut definition = f.row.definition.clone();
    definition["retrospective"] = json!({"enabled": true});
    let saved_at = f
        .persist
        .fleet()
        .save_fleet_definition(
            f.row.fleet_uuid,
            &definition,
            &f.row.missing,
            f.row.updated_at,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("retrospective fixture patch failed"))?;
    f.row = f.row.saved(definition, f.row.missing.clone(), saved_at);
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "done");
    let done = f.refresh().await?;
    assert_eq!(
        done.definition["retrospective_job"]["source_run"],
        f.run.run_id
    );
    assert_eq!(done.definition["retrospective_job"]["phase"], "pending");
    assert_eq!(done.start_report, f.row.start_report);
    assert!(done.finished_at.is_some());
    f.cleanup().await
}

async fn never_started_launch(f: &Fixture, seat: &str, minutes_old: i64) -> anyhow::Result<i64> {
    let mut launch = NewLaunch::new("herdr", f.repo.path().display().to_string());
    launch.run_id = Some(f.run.run_id.clone());
    launch.session_uuid = Some(Uuid::new_v4());
    launch.seat = Some(seat.into());
    let id = f.persist.launch().insert_launch(&launch).await?;
    sqlx::query(
        "UPDATE launch.launches SET created_at = now() - make_interval(mins => $2::int) WHERE id = $1",
    )
    .bind(id)
    .bind(i32::try_from(minutes_old)?)
    .execute(f.persist.pool())
    .await?;
    Ok(id)
}

async fn launch_state(f: &Fixture, id: i64) -> anyhow::Result<(String, Option<String>, bool)> {
    Ok(sqlx::query_as(
        "SELECT result, close_reason, closed_at IS NOT NULL FROM launch.launches WHERE id = $1",
    )
    .bind(id)
    .fetch_one(f.persist.pool())
    .await?)
}

#[tokio::test]
async fn stop_closes_a_launch_that_never_opened_a_pane_or_made_a_request() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let pending = never_started_launch(&f, "backend-hard-T2", 30).await?;
    let relaunch = never_started_launch(&f, "backend-hard-T2", 25).await?;
    sqlx::query("UPDATE launch.launches SET opened = true, result = 'failed', reason = 'mike_tools_missing' WHERE id = $1")
        .bind(relaunch)
        .execute(f.persist.pool())
        .await?;
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "done", "{report:?}");
    assert!(report.seats.iter().all(|seat| {
        seat.closed
            && seat.process_exited == Some(true)
            && seat
                .note
                .as_deref()
                .is_some_and(|note| note.starts_with("never started"))
    }));
    for id in [pending, relaunch] {
        let (result, reason, closed) = launch_state(&f, id).await?;
        assert_eq!(result, "failed");
        assert_eq!(reason.as_deref(), Some("never started"));
        assert!(closed);
    }
    f.cleanup().await
}

#[tokio::test]
async fn stop_waits_for_a_launch_that_may_still_be_opening() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let young = never_started_launch(&f, "web-T6", 0).await?;
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "stopping");
    let (result, _, closed) = launch_state(&f, young).await?;
    assert_eq!(result, "pending");
    assert!(!closed);
    f.cleanup().await
}

#[tokio::test]
async fn stop_abandons_a_blocked_receipt_that_owns_no_process_and_finishes() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let ledger = format!("{}#command:fixture", f.run.run_id);
    let receipt = f
        .persist
        .fleet()
        .begin_tree_check(&ledger, "state", "fake command", "worker")
        .await?;
    durable::leave_blocked_receipt(
        &f.run.run_id,
        &ledger,
        Some(receipt),
        "queue transaction commit outcome unknown: managed fence commit outcome unknown after timeout",
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(durable::snapshot(&f.run.run_id).pending.len(), 1);
    let report = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(report.state, "done", "{report:?}");
    assert!(durable::snapshot(&f.run.run_id).pending.is_empty());
    let row = f
        .persist
        .fleet()
        .latest_tree_check(&ledger)
        .await?
        .ok_or_else(|| anyhow::anyhow!("receipt gone"))?;
    assert_eq!(row.state_after.as_deref(), Some("abandoned:unverified"));
    assert!(row.output.contains("commit outcome unknown"));
    f.cleanup().await
}

#[path = "fleet_shutdown_more.rs"]
mod more;
