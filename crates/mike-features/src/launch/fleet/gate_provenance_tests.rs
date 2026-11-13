use super::*;
use crate::launch::fleet::managed_test_support as managed;

fn result(command: &str) -> TreeCheckRow {
    TreeCheckRow {
        id: 1,
        run_id: "fleet#gate".into(),
        state: "commit".into(),
        state_after: Some("commit".into()),
        command: command.into(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        exit_code: Some(0),
        output: String::new(),
        askers: vec![],
        told: vec![],
    }
}

#[test]
fn a_changed_command_at_the_same_commit_does_not_reuse_the_old_verdict() {
    let row = result("scripts/check-mixes");
    assert!(matches_command(&row, "scripts/check-mixes"));
    assert!(!matches_command(&row, "scripts/check-mixes --quick"));
    let row = result("scripts/check-mixes --quick");
    assert!(matches_command(&row, "scripts/check-mixes --quick"));
    assert!(!matches_command(&row, "scripts/check-mixes"));
}

#[test]
fn a_finished_report_names_the_command_that_actually_ran() {
    let mut def = FleetDefinition::default();
    def.gate.command = "scripts/check-mixes --quick".into();
    let target = Target {
        commit: "commit".into(),
        pushed: true,
        slot: "/repo".into(),
    };
    let report = finished(
        "acceptance",
        &def,
        &target,
        &result("scripts/check-mixes"),
        "finished",
    );
    assert_eq!(report.command, "scripts/check-mixes");
    assert!(report.passed);
}

#[test]
fn a_running_report_keeps_the_actual_command_commit_and_pushed_state_after_config_changes() {
    let slot = format!("/test-gate-{}", Uuid::new_v4());
    let target = Target {
        commit: "old-commit".into(),
        pushed: false,
        slot: slot.clone().into(),
    };
    let (_, receiver) = watch::channel(None);
    in_flight().insert(
        slot.clone(),
        InFlight {
            ledger: ledger("test-report"),
            target,
            command: "scripts/check-mixes".into(),
            started_at: Utc::now(),
            done: receiver,
        },
    );
    let actual = now_running(&slot);
    in_flight().remove(&slot);
    let Some(Outcome::Running {
        target,
        command,
        started_at,
        how,
    }) = actual
    else {
        panic!("inserted gate must be running");
    };
    let mut def = FleetDefinition::default();
    def.gate.command = "scripts/check-mixes --quick".into();
    let report = running_report("acceptance", &def, &target, command, started_at, how);
    assert_eq!(report.command, "scripts/check-mixes");
    assert_eq!(report.commit, "old-commit");
    assert!(!report.pushed);
    assert_eq!(report.checkout, slot);
    assert_eq!(report.how, RUNNING);
}

#[test]
fn finished_inventory_is_built_from_full_output_not_its_tail() {
    let def = FleetDefinition::default();
    let target = Target {
        commit: "commit".into(),
        pushed: true,
        slot: "/missing-gate-log".into(),
    };
    let mut row = result("gate");
    row.output = format!("test early::failure ... FAILED\n{}", "noise\n".repeat(100));
    row.exit_code = None;
    let report = finished("acceptance", &def, &target, &row, "finished");
    assert_eq!(report.failure_inventory, vec!["FAIL early::failure"]);
    assert!(!report.output_tail.contains("early::failure"));
    assert!(report.output_file.contains("persisted gate result 1"));
    assert!(text(&report).contains("did not complete (no exit code)"));
    assert!(text(&report).contains("early::failure"));
}

async fn execution_project() -> anyhow::Result<(tempfile::TempDir, FleetDefinition, Target)> {
    let root = tempfile::tempdir()?;
    let repo = root.path().to_string_lossy().into_owned();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "test"],
        vec!["commit", "--allow-empty", "-qm", "base"],
    ] {
        anyhow::ensure!(git(&repo, &args, None).await.is_some());
    }
    let commit = git(&repo, &["rev-parse", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("missing HEAD"))?;
    let target = Target {
        commit: String::from_utf8_lossy(&commit).trim().into(),
        pushed: false,
        slot: root.path().join(".claude/worktrees/mike-gate"),
    };
    let def = FleetDefinition {
        repo,
        ..FleetDefinition::default()
    };
    Ok((root, def, target))
}

#[tokio::test]
async fn dirty_source_is_unseen_by_the_gate_and_peer_work_is_untouched() -> anyhow::Result<()> {
    let (root, mut def, target) = execution_project().await?;
    std::fs::write(root.path().join("peer.txt"), "preserve me")?;
    def.gate.command = "test ! -e peer.txt && printf isolated".into();
    let (code, output, verified) = managed::gate(&def, &target, "test").await?;
    assert_eq!(code, Some(0), "{output}");
    assert!(verified);
    assert_eq!(output, "isolated");
    assert_eq!(
        std::fs::read_to_string(root.path().join("peer.txt"))?,
        "preserve me"
    );
    assert!(!target.slot.join("peer.txt").exists());
    Ok(())
}

#[tokio::test]
async fn source_changed_during_execution_cannot_be_green_commit_evidence() -> anyhow::Result<()> {
    let (root, mut def, target) = execution_project().await?;
    def.gate.command =
        "printf 'peer work' > peer.txt; printf 'test suite::early ... FAILED\\n'; exit 0".into();
    let (code, output, verified) = managed::gate(&def, &target, "test").await?;
    assert_eq!(code, None);
    assert!(!verified);
    assert!(output.contains("Original command exit: Some(0)"));
    assert_eq!(inventory(&output), vec!["FAIL suite::early"]);
    assert_eq!(
        std::fs::read_to_string(target.slot.join(OUTPUT_FILE))?,
        output
    );
    assert!(!root.path().join("peer.txt").exists());
    assert!(output.contains("put back"), "{output}");
    assert!(output.contains("peer.txt"), "{output}");
    assert!(!target.slot.join("peer.txt").exists());
    Ok(())
}

#[test]
fn gate_receipts_from_before_source_verification_cannot_be_reused() {
    assert_ne!(ledger("fleet"), "fleet#gate");
}

#[tokio::test]
async fn simultaneous_gate_asks_join_one_execution_and_keep_both_receipts() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (_root, mut def, target) = execution_project().await?;
        def.gate.command = "printf x >> .claude/worktrees/executions; sleep 0.1".into();
        let tracker = tokio_util::task::TaskTracker::new();
        let run = format!("gate-admission-{}", Uuid::new_v4());
        let fleet = managed::running_fleet(&persist, &def, &run).await?;
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run: &run,
            fleet,
        };
        let request = GateRequest {
            session: Uuid::new_v4(),
            wait_seconds: Some(10),
            status: false,
        };
        let (first, second) = tokio::join!(
            obtain(&runner, &def, ("acceptance", &request), &target),
            obtain(&runner, &def, ("acceptance", &request), &target),
        );
        let (first, second) = (
            first.map_err(anyhow::Error::msg)?,
            second.map_err(anyhow::Error::msg)?,
        );
        let (Outcome::Finished(first, _), Outcome::Finished(second, _)) = (first, second) else {
            anyhow::bail!("both requests must receive the completed execution");
        };
        assert_eq!(first.id, second.id);
        assert!(first.passed());
        tracker.close();
        tracker.wait().await;
        assert_eq!(
            std::fs::read_to_string(target.slot.join(".claude/worktrees/executions"))?,
            "x"
        );
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn gate_grades_captured_commit_after_waiting_for_the_shared_build_slot() -> anyhow::Result<()>
{
    let (root, mut def, target) = execution_project().await?;
    def.gate.command = "test ! -e peer.txt && git rev-parse HEAD".into();
    let held = super::super::gate_slot::queue::hold(root.path()).await;
    let execute = managed::gate(&def, &target, "gate-queued-source");
    let mutation = async {
        anyhow::ensure!(
            git(
                &def.repo,
                &["commit", "--allow-empty", "-qm", "advance"],
                None
            )
            .await
            .is_some()
        );
        std::fs::write(root.path().join("peer.txt"), "preserved")?;
        drop(held);
        Ok::<_, anyhow::Error>(())
    };
    let (executed, changed) = tokio::join!(execute, mutation);
    let (code, output, verified) = executed?;
    changed?;
    assert_eq!(code, Some(0), "{output}");
    assert!(verified);
    assert_eq!(output, format!("{}\n", target.commit));
    assert_eq!(
        std::fs::read_to_string(root.path().join("peer.txt"))?,
        "preserved"
    );
    Ok(())
}

#[tokio::test]
async fn gate_receipt_failure_never_starts_the_command_or_registers_a_flight() -> anyhow::Result<()>
{
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (root, mut def, target) = execution_project().await?;
        def.gate.command = "touch should-not-run".into();
        let fleet = managed::running_fleet(&persist, &def, "gate-no-receipt").await?;
        persist.pool().close().await;
        let tracker = tokio_util::task::TaskTracker::new();
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run: "gate-no-receipt",
            fleet,
        };
        let outcome = start(&runner, &def, "acceptance", &target).await;
        assert!(outcome.is_err_and(|why| why.contains("not queued")));
        tracker.close();
        tracker.wait().await;
        assert!(!root.path().join("should-not-run").exists());
        assert!(running_commit(&target.slot.to_string_lossy()).is_none());
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn identical_gate_in_another_fleet_is_busy_without_joining_its_receipt() -> anyhow::Result<()>
{
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (_root, mut def, target) = execution_project().await?;
        def.gate.command = "true".into();
        let tracker = tokio_util::task::TaskTracker::new();
        let run = format!("other-gate-{}", Uuid::new_v4());
        let fleet = managed::running_fleet(&persist, &def, &run).await?;
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run: &run,
            fleet,
        };
        let slot = admission::key(&target.slot).to_string_lossy().into_owned();
        let (_sender, receiver) = watch::channel(None);
        in_flight().insert(
            slot.clone(),
            InFlight {
                ledger: ledger("foreign-fleet"),
                target: target.clone(),
                command: def.gate.command.clone(),
                started_at: Utc::now(),
                done: receiver,
            },
        );
        let request = GateRequest {
            session: Uuid::new_v4(),
            wait_seconds: Some(0),
            status: false,
        };
        let result = obtain(&runner, &def, ("acceptance", &request), &target).await;
        in_flight().remove(&slot);
        assert!(
            result.is_err_and(
                |why| why.contains("another fleet's gate") && why.contains("not queued")
            )
        );
        assert!(
            persist
                .fleet()
                .latest_tree_check(&ledger(&run))
                .await?
                .is_none()
        );
        assert!(!waiting().contains_key(&slot));
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn interrupted_gate_receipt_is_not_reported_as_never_run() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let ledger = ledger(&format!("interrupted-gate-{}", Uuid::new_v4()));
        let id = persist
            .fleet()
            .begin_tree_check(&ledger, "commit", "true", "acceptance")
            .await?;
        let status = current_status(&persist, &ledger, "/missing-flight-slot").await;
        assert!(status.is_err_and(
            |why| why.contains(&format!("receipt {id}")) && why.contains("interrupted")
        ));
        db.cleanup().await?;
        Ok(())
    })
    .await?
}

#[tokio::test]
async fn gate_reuses_its_build_directory_and_preserves_staged_peer_source() -> anyhow::Result<()> {
    let (root, mut def, mut target) = execution_project().await?;
    std::fs::write(root.path().join("source.txt"), "committed")?;
    std::fs::write(root.path().join(".gitignore"), "target/\n")?;
    anyhow::ensure!(
        git(&def.repo, &["add", "source.txt", ".gitignore"], None)
            .await
            .is_some()
    );
    anyhow::ensure!(
        git(&def.repo, &["commit", "-qm", "source"], None)
            .await
            .is_some()
    );
    let commit = git(&def.repo, &["rev-parse", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("no HEAD"))?;
    target.commit = String::from_utf8_lossy(&commit).trim().into();
    def.gate.command = "cat source.txt; test ! -e peer.txt".into();
    std::fs::write(root.path().join("source.txt"), "staged")?;
    anyhow::ensure!(git(&def.repo, &["add", "source.txt"], None).await.is_some());
    std::fs::write(root.path().join("source.txt"), "unstaged")?;
    std::fs::write(root.path().join("peer.txt"), "untracked")?;
    let before = git(&def.repo, &["status", "--porcelain"], None).await;
    let first = managed::gate(&def, &target, "warm-gate").await?;
    assert_eq!(first, (Some(0), "committed".into(), true));
    std::fs::create_dir_all(target.slot.join("target"))?;
    std::fs::write(target.slot.join("target/warm"), "cached")?;
    def.gate.command = "cat source.txt target/warm".into();
    let second = managed::gate(&def, &target, "warm-gate").await?;
    assert_eq!(second, (Some(0), "committedcached".into(), true));
    assert_eq!(
        git(&def.repo, &["status", "--porcelain"], None).await,
        before
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("source.txt"))?,
        "unstaged"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("peer.txt"))?,
        "untracked"
    );
    assert_eq!(
        std::fs::read_to_string(target.slot.join(OUTPUT_FILE))?,
        "committedcached"
    );
    Ok(())
}
