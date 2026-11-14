use super::*;
use crate::launch::fleet::definition::{TaskDefinition, WorkerDefinition};
use crate::launch::fleet::managed_test_support as managed;
use crate::store::persistence::launch_repo::LaunchPersist as _;

fn red() -> FleetDefinition {
    let mut def = FleetDefinition::default();
    def.gate.command = "false".to_string();
    def.baseline = Some(Baseline {
        command: "false".to_string(),
        verified: true,
        ..Baseline::default()
    });
    def.workers
        .insert("rust".to_string(), WorkerDefinition::default());
    def.tasks.insert(
        "repair".to_string(),
        TaskDefinition {
            discipline: "rust".to_string(),
            ..TaskDefinition::default()
        },
    );
    def
}

fn ready() -> Vec<String> {
    vec!["ordinary".to_string(), "repair".to_string()]
}

#[test]
fn red_baseline_holds_workers_until_an_explicit_repair_owner_exists() -> anyhow::Result<()> {
    let mut def = red();
    assert!(blocked(&def));
    assert!(admitted(&def, ready(), false).is_empty());
    def.baseline
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture baseline"))?
        .repair_task = Some("repair".to_string());
    assert_eq!(admitted(&def, ready(), false), vec!["repair"]);
    assert_eq!(admitted(&def, ready(), true), ready());
    Ok(())
}

#[test]
fn a_red_baseline_holds_only_the_tasks_whose_files_touch_the_repair() -> anyhow::Result<()> {
    let mut def = red();
    let task = |files: &[&str]| TaskDefinition {
        discipline: "rust".to_string(),
        files: files.iter().map(ToString::to_string).collect(),
        ..TaskDefinition::default()
    };
    def.tasks
        .insert("repair".into(), task(&["ink/package.json", "src/gate/"]));
    def.tasks.insert("apart".into(), task(&["crates/core/"]));
    def.tasks
        .insert("inside".into(), task(&["src/gate/run.rs"]));
    def.tasks.insert("unknown".into(), task(&[]));
    def.baseline
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture baseline"))?
        .repair_task = Some("repair".to_string());
    let ready: Vec<String> = ["apart", "inside", "repair", "unknown"]
        .map(String::from)
        .to_vec();
    assert_eq!(admitted(&def, ready, false), vec!["apart", "repair"]);
    Ok(())
}

#[test]
fn malformed_or_dependent_repair_owner_is_rejected() -> anyhow::Result<()> {
    let mut def = red();
    def.baseline
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture baseline"))?
        .repair_task = Some("unknown".to_string());
    assert!(repair_task(&def).is_err());
    def.baseline
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture baseline"))?
        .repair_task = Some("repair".to_string());
    def.tasks
        .get_mut("repair")
        .ok_or_else(|| anyhow::anyhow!("fixture task"))?
        .after
        .push("ordinary".to_string());
    assert!(repair_task(&def).is_err());
    assert!(admitted(&def, ready(), false).is_empty());
    def.tasks
        .get_mut("repair")
        .ok_or_else(|| anyhow::anyhow!("fixture task"))?
        .after
        .clear();
    def.workers.clear();
    assert!(repair_task(&def).is_err());
    Ok(())
}

#[test]
fn legacy_fleets_and_verified_green_baselines_admit_normal_workers() -> anyhow::Result<()> {
    assert_eq!(
        admitted(&FleetDefinition::default(), ready(), false),
        ready()
    );
    let mut def = red();
    def.baseline
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture baseline"))?
        .passed = true;
    assert_eq!(admitted(&def, ready(), false), ready());
    def.gate.command = "changed gate".to_string();
    assert!(admitted(&def, ready(), false).is_empty());
    def.baseline
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("fixture baseline"))?
        .verified = false;
    assert!(admitted(&def, ready(), false).is_empty());
    Ok(())
}

async fn project() -> anyhow::Result<(tempfile::TempDir, FleetDefinition)> {
    let root = tempfile::tempdir()?;
    let repo = root.path().to_string_lossy().to_string();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@example.invalid"],
        vec!["config", "user.name", "Test"],
    ] {
        git(&repo, &args).await.map_err(anyhow::Error::msg)?;
    }
    std::fs::write(root.path().join("source"), "original")?;
    git(&repo, &["add", "source"])
        .await
        .map_err(anyhow::Error::msg)?;
    git(&repo, &["commit", "-m", "fixture"])
        .await
        .map_err(anyhow::Error::msg)?;
    Ok((
        root,
        FleetDefinition {
            repo,
            branch: "main".to_string(),
            ..FleetDefinition::default()
        },
    ))
}

#[tokio::test]
async fn clean_baseline_uses_existing_checkout_and_preserves_dirty_files() -> anyhow::Result<()> {
    let (root, def) = project().await?;
    assert!(clean_commit(&def).await.is_ok());
    std::fs::write(root.path().join("source"), "keep tracked edits")?;
    std::fs::write(root.path().join("untracked"), "keep untracked edits")?;
    assert!(clean_commit(&def).await.is_err());
    assert_eq!(
        std::fs::read_to_string(root.path().join("source"))?,
        "keep tracked edits"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("untracked"))?,
        "keep untracked edits"
    );
    assert!(!root.path().join(".claude/worktrees/mike-check").exists());
    Ok(())
}

#[tokio::test]
async fn missing_repository_and_other_branch_fail_closed() -> anyhow::Result<()> {
    let (root, mut def) = project().await?;
    def.branch = "absent".to_string();
    assert!(clean_commit(&def).await.is_err());
    def.repo = root.path().join("absent").to_string_lossy().to_string();
    assert!(clean_commit(&def).await.is_err());
    Ok(())
}

#[tokio::test]
async fn simultaneous_startups_cannot_share_the_baseline_slot() -> anyhow::Result<()> {
    let (_root, def) = project().await?;
    super::super::gate_slot::prepare(&def.repo)
        .await
        .map_err(anyhow::Error::msg)?;
    let held = startup_lock(&def.repo).await.map_err(anyhow::Error::msg)?;
    assert!(startup_lock(&def.repo).await.is_err());
    drop(held);
    assert!(startup_lock(&def.repo).await.is_ok());
    Ok(())
}

async fn starting_fleet(
    persist: &PersistHandle,
    def: &FleetDefinition,
) -> anyhow::Result<FleetFacts> {
    use crate::store::persistence::fleet::NewFleet;
    use crate::store::persistence::launch::NewRun;
    let unique = uuid::Uuid::new_v4();
    let run_id = format!("startup-isolation-{unique}");
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
            slug: format!("startup-isolation-{unique}"),
            project_path: def.repo.clone(),
            definition: serde_json::to_value(def)?,
            missing: json!([]),
            created_by: None,
        })
        .await?;
    crate::store::persistence::test_support::seed_fleet_starting(
        persist,
        fleet.fleet_uuid,
        Some(&run_id),
    )
    .await?;
    Ok(FleetFacts {
        fleet: fleet.fleet_uuid.to_string(),
        run_id,
        ..super::super::plan_tests::facts()
    })
}

#[tokio::test]
async fn startup_and_ordinary_gates_keep_their_own_commits_in_the_same_warm_slot()
-> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (_root, mut def) = project().await?;
        let first = super::super::gate::Target {
            commit: clean_commit(&def).await.map_err(anyhow::Error::msg)?,
            pushed: false,
            slot: super::super::gate_slot::slot_for(&def.repo)
                .ok_or_else(|| anyhow::anyhow!("no slot"))?,
        };
        std::fs::write(std::path::Path::new(&def.repo).join("source"), "next")?;
        git(&def.repo, &["commit", "-am", "next"])
            .await
            .map_err(anyhow::Error::msg)?;
        let second = super::super::gate::Target {
            commit: clean_commit(&def).await.map_err(anyhow::Error::msg)?,
            ..first.clone()
        };
        def.gate.command = "cat source; sleep 0.1; cat source; git rev-parse HEAD".into();
        let facts = starting_fleet(&persist, &def).await?;
        let tracker = tokio_util::task::TaskTracker::new();
        let ordinary_run = format!("ordinary-gate-{}", uuid::Uuid::new_v4());
        let fleet = managed::running_fleet(&persist, &def, &ordinary_run).await?;
        let runner = super::super::check::Runner {
            persist: &persist,
            tracker: &tracker,
            run: &ordinary_run,
            fleet,
            decisions: None,
        };
        let (startup, ordinary) = tokio::join!(
            managed::startup_gate(&persist, &def, &facts, &first, &tracker),
            managed::durable_gate(&runner, &def, &second)
        );
        tracker.close();
        tracker.wait().await;
        assert_eq!(
            startup?,
            (Some(0), format!("originaloriginal{}\n", first.commit), true)
        );
        assert_eq!(
            ordinary?,
            (Some(0), format!("nextnext{}\n", second.commit), true)
        );
        assert_eq!(first.slot, second.slot);
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn startup_source_verification_preserves_clean_admission_and_rejects_mutation()
-> anyhow::Result<()> {
    let (root, def) = project().await?;
    let commit = clean_commit(&def).await.map_err(anyhow::Error::msg)?;
    let mut output = String::new();
    assert!(source_unchanged(&def, &commit, true, &mut output).await);
    assert!(!source_unchanged(&def, &commit, false, &mut output).await);
    assert!(!source_unchanged(&def, "another-commit", true, &mut output).await);
    assert!(output.contains("changed commits"));
    std::fs::write(root.path().join("source"), "changed")?;
    assert!(!source_unchanged(&def, &commit, true, &mut output).await);
    assert!(output.contains("has changes"));
    assert_eq!(
        std::fs::read_to_string(root.path().join("source"))?,
        "changed"
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_startup_cannot_complete_a_gate_verdict() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (_root, mut def) = project().await?;
        def.gate.command = "sleep 60".into();
        let facts = starting_fleet(&persist, &def).await?;
        persist
            .fleet()
            .set_fleet_state(facts.fleet.parse()?, "stopping", Some(&facts.run_id), None)
            .await?;
        let target = super::super::gate::Target {
            commit: clean_commit(&def).await.map_err(anyhow::Error::msg)?,
            pushed: false,
            slot: super::super::gate_slot::slot_for(&def.repo)
                .ok_or_else(|| anyhow::anyhow!("no slot"))?,
        };
        let tracker = tokio_util::task::TaskTracker::new();
        let (code, output, verified) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            managed::startup_gate(&persist, &def, &facts, &target, &tracker),
        )
        .await??;
        tracker.close();
        tracker.wait().await;
        assert_eq!(code, None);
        assert!(!verified);
        assert!(
            output.contains("fleet state or exact run no longer admits execution"),
            "{output}"
        );
        assert!(
            persist
                .fleet()
                .latest_tree_check(&super::super::gate::ledger(&facts.run_id))
                .await?
                .is_none()
        );
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[test]
fn a_red_baseline_without_a_repair_task_names_the_result_and_what_to_set() {
    let why = red_without_repair("uv run pytest", 1739, &"ledger 7");
    assert!(why.contains("`uv run pytest`"), "{why}");
    assert!(why.contains("1739"), "{why}");
    assert!(why.contains("baseline.repair_task"), "{why}");
    assert!(why.contains("send ready again"), "{why}");
}
