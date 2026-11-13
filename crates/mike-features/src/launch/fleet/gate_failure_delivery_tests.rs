use super::*;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::{NewLaunch, NewRun};
use crate::store::persistence::test_support::seed_fleet_running;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

#[test]
fn ownership_notice_keeps_timeout_command_commit_log_and_authorship_uncertainty() {
    let text = heading("exact gate", "full-sha", None, Path::new("/full/log"));
    for detail in [
        "exact gate",
        "full-sha",
        "timeout",
        "/full/log",
        "does not establish who introduced",
        "persisted gate result",
    ] {
        assert!(text.contains(detail), "{text}");
    }
    assert!(!text.contains("Your commit"));
}

struct Fixture {
    db: TestDbHandle,
    persist: PersistHandle,
    run: String,
    dir: tempfile::TempDir,
    def: FleetDefinition,
    target: Target,
    old: Uuid,
    successor: Uuid,
    other: Uuid,
    coordinator: Uuid,
    acceptance: Uuid,
}

async fn checkout() -> anyhow::Result<(tempfile::TempDir, Target)> {
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("ink"))?;
    for file in ["ink/a.test.ts", "ink/b.test.ts", "unowned.test.ts"] {
        std::fs::write(dir.path().join(file), "test fixture")?;
    }
    let repo = dir.path().to_string_lossy();
    for args in [
        vec!["init", "-q"],
        vec!["add", "ink/a.test.ts", "ink/b.test.ts", "unowned.test.ts"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        anyhow::ensure!(git(&repo, &args, None).await.is_some());
    }
    let sha = git(&repo, &["rev-parse", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("fixture commit missing"))?;
    let target = Target {
        commit: String::from_utf8(sha)?.trim().into(),
        pushed: true,
        slot: dir.path().into(),
    };
    Ok((dir, target))
}

async fn launch(
    persist: &PersistHandle,
    run: &str,
    session: Uuid,
    seat: &str,
) -> anyhow::Result<i64> {
    persist
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/gate-owner-test");
    launch.run_id = Some(run.into());
    launch.session_uuid = Some(session);
    launch.seat = Some(seat.into());
    Ok(persist.launch().insert_launch(&launch).await?)
}

async fn fixture(replace: bool) -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let run = format!("gate-owner-{}", Uuid::new_v4());
    let (dir, target) = checkout().await?;
    let repo = dir.path().to_string_lossy().into_owned();
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run.clone(),
            project_path: Some(repo.clone()),
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let (old, successor, other, coordinator, acceptance) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let old_launch = launch(&persist, &run, old, "worker-A").await?;
    for (session, seat) in [
        (successor, "worker-A"),
        (other, "worker-B"),
        (coordinator, "coordinator"),
        (acceptance, "acceptance"),
    ] {
        launch(&persist, &run, session, seat).await?;
    }
    let def: FleetDefinition = serde_json::from_value(serde_json::json!({
        "repo": repo, "gate": {"command": "project test command"},
        "coordinator": {"session": coordinator.to_string()},
        "acceptance": {"session": acceptance.to_string()},
        "tasks": {
            "A": {"seat": "worker-A", "session": old.to_string(), "files": ["ink/a.test.ts"]},
            "B": {"seat": "worker-B", "session": other.to_string(), "files": ["ink/b.test.ts"]}
        }
    }))?;
    let mut current = def.clone();
    if replace {
        current.tasks.entry("A".into()).or_default().session = Some(successor.to_string());
    }
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run.clone(),
            project_path: repo,
            definition: serde_json::to_value(current)?,
            missing: serde_json::json!([]),
            created_by: None,
        })
        .await?;
    seed_fleet_running(&persist, row.fleet_uuid, &run).await?;
    persist
        .launch()
        .record_launch_closed(old_launch, "replaced", Some(true))
        .await?;
    Ok(Some(Fixture {
        db,
        persist,
        run,
        dir,
        def,
        target,
        old,
        successor,
        other,
        coordinator,
        acceptance,
    }))
}

async fn inbox(persist: &PersistHandle, session: Uuid) -> anyhow::Result<Vec<String>> {
    Ok(persist
        .launch()
        .fetch_for_session_with_activity(session)
        .await?
        .inbox
        .into_iter()
        .map(|message| message.row.body)
        .collect())
}

#[tokio::test]
async fn timed_out_gate_mails_each_owner_current_successor_and_unresolved_to_coordinator()
-> anyhow::Result<()> {
    let Some(f) = fixture(true).await? else {
        return Ok(());
    };
    let log = f.dir.path().join("gate.log");
    let output = "FAIL a.test.ts > first\nFAIL b.test.ts > second\nFAIL unowned.test.ts > investigate\nFAIL binary unknown_test\nthe gate was stopped after 120 seconds";
    std::fs::write(&log, output)?;
    crate::launch::fleet::gate_queue::after_run(
        &f.persist,
        &f.def,
        &f.run,
        &f.target,
        (None, output, &log, true),
    )
    .await;
    for (session, evidence) in [(f.successor, "first"), (f.other, "second")] {
        let messages = inbox(&f.persist, session).await?;
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("File ownership contacts"));
        assert!(messages[0].contains(evidence));
        assert!(messages[0].contains("does not establish who introduced"));
        assert!(messages[0].contains("timeout"));
    }
    assert!(inbox(&f.persist, f.old).await?.is_empty());
    let lead = inbox(&f.persist, f.coordinator).await?.join("\n");
    assert!(lead.contains("unowned.test.ts"));
    assert!(lead.contains("unknown_test"));
    assert!(lead.contains("ownership unknown"));
    let acceptance = inbox(&f.persist, f.acceptance).await?.join("\n");
    assert!(acceptance.contains("first"));
    assert!(acceptance.contains("second"));
    assert!(acceptance.contains("not completed"));
    f.db.cleanup().await
}

#[tokio::test]
async fn closed_owner_without_successor_falls_back_to_coordinator_and_preserves_receipt()
-> anyhow::Result<()> {
    let Some(f) = fixture(false).await? else {
        return Ok(());
    };
    let output = "FAIL a.test.ts > still unresolved";
    let ledger = crate::launch::fleet::gate::ledger(&f.run);
    let id = f
        .persist
        .fleet()
        .begin_tree_check(&ledger, &f.target.commit, &f.def.gate.command, "mike")
        .await?;
    f.persist
        .fleet()
        .finish_tree_check(id, None, output, Some(&f.target.commit))
        .await?;
    let sent = tell_owners(
        &f.persist,
        &f.def,
        &f.run,
        &f.target,
        (None, output, &f.dir.path().join("missing.log")),
    )
    .await;
    assert!(sent.contains(&f.coordinator));
    assert!(!sent.contains(&f.old));
    assert!(inbox(&f.persist, f.old).await?.is_empty());
    assert!(inbox(&f.persist, f.successor).await?.is_empty());
    let lead = inbox(&f.persist, f.coordinator).await?.join("\n");
    assert!(lead.contains("is closed"));
    assert!(lead.contains("still unresolved"));
    let row = f
        .persist
        .fleet()
        .find_tree_check(&ledger, &f.target.commit)
        .await?
        .ok_or_else(|| anyhow::anyhow!("durable receipt missing"))?;
    assert_eq!(row.output, output);
    f.db.cleanup().await
}

#[tokio::test]
async fn mailbox_failure_keeps_full_receipt_and_reports_owner_delivery_problem()
-> anyhow::Result<()> {
    let Some(f) = fixture(false).await? else {
        return Ok(());
    };
    let mut def = f.def.clone();
    let missing = Uuid::new_v4();
    def.tasks.entry("B".into()).or_default().session = Some(missing.to_string());
    let fleet = f
        .persist
        .fleet()
        .find_fleet_for_session(f.coordinator)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing"))?;
    anyhow::ensure!(
        f.persist
            .fleet()
            .save_fleet_definition(
                fleet.fleet_uuid,
                &serde_json::to_value(&def)?,
                &serde_json::json!([]),
                fleet.updated_at,
            )
            .await?
            .is_some()
    );
    let output = "FAIL b.test.ts > missing recipient";
    let ledger = crate::launch::fleet::gate::ledger(&f.run);
    let id = f
        .persist
        .fleet()
        .begin_tree_check(&ledger, &f.target.commit, &def.gate.command, "mike")
        .await?;
    f.persist
        .fleet()
        .finish_tree_check(id, None, output, Some(&f.target.commit))
        .await?;
    let sent = tell_owners(
        &f.persist,
        &def,
        &f.run,
        &f.target,
        (None, output, &f.dir.path().join("missing.log")),
    )
    .await;
    assert!(sent.is_empty());
    let lead = inbox(&f.persist, f.coordinator).await?.join("\n");
    assert!(lead.contains("missing or closed"));
    assert!(lead.contains(&missing.to_string()));
    assert!(lead.contains("missing recipient"));
    assert!(lead.contains("persisted gate result"));
    assert!(inbox(&f.persist, f.other).await?.is_empty());
    let row = f
        .persist
        .fleet()
        .find_tree_check(&ledger, &f.target.commit)
        .await?
        .ok_or_else(|| anyhow::anyhow!("durable receipt missing"))?;
    assert_eq!(row.output, output);
    f.db.cleanup().await
}

#[tokio::test]
async fn successful_or_unverified_gate_never_assigns_file_owner_repairs() -> anyhow::Result<()> {
    let Some(f) = fixture(true).await? else {
        return Ok(());
    };
    let log = f.dir.path().join("gate.log");
    for (code, verified) in [(Some(0), true), (None, false)] {
        crate::launch::fleet::gate_queue::after_run(
            &f.persist,
            &f.def,
            &f.run,
            &f.target,
            (code, "FAIL a.test.ts > observed", &log, verified),
        )
        .await;
    }
    assert!(inbox(&f.persist, f.old).await?.is_empty());
    assert!(inbox(&f.persist, f.successor).await?.is_empty());
    assert!(inbox(&f.persist, f.other).await?.is_empty());
    assert!(inbox(&f.persist, f.coordinator).await?.is_empty());
    assert!(
        inbox(&f.persist, f.acceptance)
            .await?
            .iter()
            .any(|body| body.contains("Source unverified"))
    );
    f.db.cleanup().await
}

#[test]
fn noisy_notice_lists_fifty_entries_counts_all_omissions_and_keeps_full_output_reference() {
    let failures: BTreeSet<_> = (0..75).map(|n| format!("failure {n:03}")).collect();
    let text = format!(
        "{}\n{}",
        heading("gate", "sha", None, Path::new("/exact/log")),
        listed(&failures)
    );
    assert_eq!(
        text.lines().filter(|line| line.starts_with("- ")).count(),
        50
    );
    assert!(text.contains("25 additional detail entries omitted"));
    assert!(text.contains("failure 000"));
    assert!(text.contains("failure 049"));
    assert!(!text.contains("failure 050"));
    assert!(text.contains("/exact/log"));
    assert!(text.contains("persisted gate result"));
    assert_eq!(failures.len(), 75);
}

#[test]
fn notice_utf8_byte_budget_omits_whole_entries_and_reports_exact_remaining_count() {
    let failures: BTreeSet<_> = (0..40)
        .map(|n| format!("{n:03} {}", "界".repeat(100)))
        .collect();
    let text = listed(&failures);
    let shown: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .collect();
    assert!(text.len() <= NOTICE_MAX_BYTES);
    assert!(!shown.is_empty());
    assert!(shown.len() < failures.len());
    assert!(shown.iter().all(|line| failures.contains(*line)));
    assert!(text.contains(&format!(
        "{} additional detail entries omitted",
        failures.len() - shown.len()
    )));
}

#[test]
fn oversized_unicode_detail_is_omitted_whole_without_losing_durable_lookup() {
    let failures = BTreeSet::from(["界".repeat(NOTICE_MAX_BYTES)]);
    let text = listed(&failures);
    assert!(text.len() <= NOTICE_MAX_BYTES);
    assert!(text.contains("1 additional detail entries omitted"));
    assert!(text.contains("captured gate log or persisted result"));
    assert!(!text.contains('界'));
}

#[test]
fn ordinary_command_is_exact_and_long_unicode_command_is_bounded_with_durable_escape() {
    assert_eq!(notice_command("cd ink && npm test"), "cd ink && npm test");
    let prefix = "a".repeat(NOTICE_COMMAND_BYTES - 1);
    let command = format!("{prefix}🧪{}", "界".repeat(10_000));
    let text = heading(&command, "sha", None, Path::new("/exact/log"));
    assert!(text.len() < NOTICE_COMMAND_BYTES + 1024);
    assert!(text.contains(&prefix));
    assert!(!text.contains('🧪'));
    assert!(text.contains(&format!(
        "command truncated: {} UTF-8 bytes omitted",
        command.len() - prefix.len()
    )));
    assert!(text.contains("exact command in the persisted gate result"));
    assert!(text.contains("/exact/log"));
}
