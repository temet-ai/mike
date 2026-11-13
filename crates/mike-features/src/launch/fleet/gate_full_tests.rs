use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use super::super::super::gate_slot::slot_for;
use super::super::super::managed_test_support as managed;
use super::super::super::operation::Operation;
use super::super::{Target, ledger_for};
use super::*;

const QUICK: &str = "echo quick; git rev-parse HEAD";
const FULL: &str = "echo full; git rev-parse HEAD";

fn quick_and_full() -> FleetDefinition {
    let mut def = FleetDefinition::default();
    def.gate.command = QUICK.to_string();
    def.gate.full_command = FULL.to_string();
    def
}

#[test]
fn the_full_definition_runs_full_command_on_its_own_ledger() {
    let def = quick_and_full();
    let full = definition(&def);
    assert_eq!(full.gate.command, FULL);
    assert_eq!(command(&def), FULL);
    assert_ne!(ledger_for(&def, "run"), ledger_for(&full, "run"));
    assert!(ledger_for(&full, "run").ends_with(LEDGER_SUFFIX));
    let mut alone = quick_and_full();
    alone.gate.full_command = String::new();
    assert_eq!(definition(&alone).gate.command, QUICK);
    assert_eq!(command(&alone), QUICK);
    assert_eq!(ledger_for(&alone, "run"), super::super::ledger("run"));
}

#[test]
fn a_gate_command_that_names_the_full_gate_is_warned_about() {
    let mut def = quick_and_full();
    def.gate.full_command = String::new();
    def.execution.operations.insert(
        "gate".to_string(),
        Operation {
            command: QUICK.to_string(),
            purpose: "Full project acceptance gate".to_string(),
            ..Operation::default()
        },
    );
    let found = warnings(&def);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].item, "gate.command");
    assert!(found[0].problem.contains("gate.full_command"), "{found:?}");
    def.gate.full_command = FULL.to_string();
    assert!(warnings(&def).is_empty());
}

#[test]
fn a_long_measured_or_long_declared_gate_is_a_full_gate() {
    let mut def = quick_and_full();
    def.gate.full_command = String::new();
    assert!(full_gate_reason(&def, Some(Duration::from_secs(60))).is_none());
    assert!(full_gate_reason(&def, Some(Duration::from_secs(27 * 60))).is_some());
    def.execution.operations.insert(
        "quick".to_string(),
        Operation {
            command: QUICK.to_string(),
            purpose: "Format and compile".to_string(),
            timeout_seconds: Some(3600),
            ..Operation::default()
        },
    );
    assert!(full_gate_reason(&def, None).is_some());
}

async fn project() -> anyhow::Result<(tempfile::TempDir, FleetDefinition)> {
    let root = tempfile::tempdir()?;
    let repo = root.path().to_string_lossy().to_string();
    let git = |args: &'static [&'static str]| {
        let repo = repo.clone();
        async move {
            super::super::super::watch::git(&repo, args, None)
                .await
                .ok_or_else(|| anyhow::anyhow!("git {args:?} failed"))
        }
    };
    git(&["init", "-b", "main"]).await?;
    git(&["config", "user.email", "test@example.invalid"]).await?;
    git(&["config", "user.name", "Test"]).await?;
    std::fs::write(root.path().join("source"), "original")?;
    git(&["add", "source"]).await?;
    git(&["commit", "-m", "fixture"]).await?;
    let mut def = quick_and_full();
    def.repo = repo;
    def.branch = "main".to_string();
    Ok((root, def))
}

#[tokio::test]
async fn startup_and_pushes_run_command_and_close_runs_full_command_once() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (_root, def) = project().await?;
        let target = commit_to_check(&def)
            .await
            .ok_or_else(|| anyhow::anyhow!("no commit to check"))?;
        assert_eq!(Some(target.slot.clone()), slot_for(&def.repo));
        let run = format!("full-gate-{}", uuid::Uuid::new_v4());
        let fleet = managed::running_fleet(&persist, &def, &run).await?;
        let tracker = TaskTracker::new();
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run: &run,
            fleet,
        };
        let (_, quick, verified) = managed::durable_gate(&runner, &def, &target).await?;
        assert!(verified);
        assert_eq!(quick, format!("quick\n{}\n", target.commit));
        let row = persist
            .fleet()
            .find_fleet(&fleet.to_string())
            .await?
            .ok_or_else(|| anyhow::anyhow!("fleet row"))?;
        assert_eq!(
            unchecked_at_close(&tracker, &persist, &row).await,
            Some(target.commit.clone()),
            "the full gate has not run yet, so close waits for it"
        );
        tracker.close();
        tracker.wait().await;
        let full = definition(&def);
        let ran = persist
            .fleet()
            .find_tree_check(&ledger_for(&full, &run), &target.commit)
            .await?
            .ok_or_else(|| anyhow::anyhow!("close did not run the full gate"))?;
        assert_eq!(ran.command, FULL);
        assert_eq!(ran.output, format!("full\n{}\n", target.commit));
        assert!(ran.passed());
        assert_eq!(unchecked_at_close(&tracker, &persist, &row).await, None);
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

async fn starting_fleet(
    persist: &PersistHandle,
    def: &FleetDefinition,
) -> anyhow::Result<super::super::super::plan::FleetFacts> {
    use crate::store::persistence::fleet::NewFleet;
    use crate::store::persistence::launch::NewRun;
    let run_id = format!("full-gate-startup-{}", uuid::Uuid::new_v4());
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run_id.clone(),
            project_path: Some(def.repo.clone()),
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let fleet = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run_id.clone(),
            project_path: def.repo.clone(),
            definition: serde_json::to_value(def)?,
            missing: serde_json::json!([]),
            created_by: None,
        })
        .await?;
    crate::store::persistence::test_support::seed_fleet_starting(
        persist,
        fleet.fleet_uuid,
        Some(&run_id),
    )
    .await?;
    Ok(super::super::super::plan::FleetFacts {
        fleet: fleet.fleet_uuid.to_string(),
        run_id,
        ..super::super::super::plan_tests::facts()
    })
}

#[tokio::test]
async fn the_startup_baseline_runs_command_not_full_command() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (_root, def) = project().await?;
        let target: Target = commit_to_check(&def)
            .await
            .ok_or_else(|| anyhow::anyhow!("no commit to check"))?;
        let facts = starting_fleet(&persist, &def).await?;
        let tracker = TaskTracker::new();
        let (code, output, verified) =
            managed::startup_gate(&persist, &def, &facts, &target, &tracker).await?;
        tracker.close();
        tracker.wait().await;
        assert_eq!(code, Some(0));
        assert!(verified);
        assert_eq!(output, format!("quick\n{}\n", target.commit));
        db.cleanup().await?;
        Ok(())
    })
    .await?
}
