use super::*;
use crate::launch::fleet::check::Asker;
use crate::launch::fleet::check_run::baseline;
use crate::launch::fleet::gate_slot::{isolated, slot_for};
use std::time::Duration;

#[tokio::test]
async fn configured_check_preserves_staged_unstaged_and_untracked_peer_files() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    std::fs::write(repo.join("mine/a.txt"), "staged\n")?;
    git(&repo, &["add", "mine/a.txt"]).await?;
    std::fs::write(repo.join("mine/a.txt"), "unstaged\n")?;
    std::fs::write(repo.join("theirs/new.txt"), "untracked\n")?;
    std::fs::create_dir_all(repo.join("target"))?;
    std::fs::write(repo.join("target/shared"), "do not copy")?;
    let before = git(&repo, &["status", "--porcelain"]).await?;
    let index = git(&repo, &["show", ":mine/a.txt"]).await?;
    def.check.command = "cat mine/a.txt; test ! -e theirs/new.txt && test ! -e target/shared && printf '%s\\n' \"$MIKE_RUN_ID\"".into();
    let (code, output) = managed::check(&def, &at, "fleet-isolated").await?;
    assert_eq!((code, output.as_str()), (Some(0), "ok\nfleet-isolated\n"));
    assert_eq!(git(&repo, &["status", "--porcelain"]).await?, before);
    assert_eq!(git(&repo, &["show", ":mine/a.txt"]).await?, index);
    assert_eq!(
        std::fs::read_to_string(repo.join("mine/a.txt"))?,
        "unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("theirs/new.txt"))?,
        "untracked\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("target/shared"))?,
        "do not copy"
    );
    assert!(!baseline::is_baseline("fleet-isolated"));
    Ok(())
}

#[tokio::test]
async fn a_check_that_writes_source_is_named_and_the_next_check_still_runs() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_repo, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    def.check.command = "printf rewritten > mine/a.txt; printf new > stray.txt".into();
    let (code, output) = managed::check(&def, &at, "writes-source").await?;
    assert_eq!(code, None);
    assert!(output.contains("put back"), "{output}");
    assert!(output.contains("mine/a.txt"), "{output}");
    def.check.command = "cat mine/a.txt; test ! -e stray.txt".into();
    let (code, output) = managed::check(&def, &at, "after-a-writer").await?;
    assert_eq!((code, output.as_str()), (Some(0), "ok\n"));
    Ok(())
}

#[tokio::test]
async fn configured_and_baseline_checks_reuse_one_warm_slot_without_switching_a_running_commit()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    let first = head(&def).await.map_err(anyhow::Error::msg)?;
    let (one, other) = tokio::join!(
        managed::check(&def, &first, "warm-slot"),
        managed::check(&def, &first, "warm-slot-other")
    );
    assert_eq!((one?.0, other?.0), (Some(0), Some(0)));
    for name in crate::launch::fleet::gate_slot::CHECK_SLOT_NAMES {
        std::fs::write(
            first
                .slot
                .with_file_name(name)
                .join(".claude/worktrees/warm"),
            "cached",
        )?;
    }
    seat_commit(&repo, "mine/a.txt", "second\n").await?;
    let second = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    def.check.command = "cat mine/a.txt; sleep 0.1; cat mine/a.txt .claude/worktrees/warm".into();
    let (automatic, baseline) = tokio::join!(
        managed::check(&def, &first, "warm-slot"),
        managed::check(&def, &second, "warm-slot#baseline:second")
    );
    assert_eq!(automatic?, (Some(0), "ok\nok\ncached".into()));
    assert_eq!(baseline?, (Some(0), "second\nsecond\ncached".into()));
    assert_eq!(first.slot, second.slot);
    assert_eq!(
        std::fs::read_to_string(first.slot.join(".claude/worktrees/warm"))?,
        "cached"
    );
    Ok(())
}

#[tokio::test]
async fn isolated_checks_and_gates_share_the_preparation_execution_and_verification_lock()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_repo, def) = project(root.path()).await?;
    let first = head(&def).await.map_err(anyhow::Error::msg)?;
    let second = crate::launch::fleet::gate::Target {
        slot: slot_for(&def.repo).ok_or_else(|| anyhow::anyhow!("no gate slot"))?,
        ..first.clone()
    };
    let command = "test ! -e .in-flight || exit 9; touch .in-flight; sleep 0.1; rm .in-flight; git rev-parse HEAD";
    let check = |name: &'static str| {
        managed::with_context(
            name,
            isolated::execute(
                &def.repo,
                &first,
                name,
                command,
                isolated::Operation::Check {
                    timeout: Duration::from_secs(5),
                    configured: true,
                },
            ),
        )
    };
    let (one, two, gate) = tokio::join!(
        check("check-lock-1"),
        check("check-lock-2"),
        managed::with_context(
            "gate-lock",
            isolated::execute(
                &def.repo,
                &second,
                "gate-lock",
                command,
                isolated::Operation::Gate {
                    completion_log: None
                }
            )
        )
    );
    assert_eq!(one?, (Some(0), format!("{}\n", first.commit), true));
    assert_eq!(two?, (Some(0), format!("{}\n", first.commit), true));
    assert_eq!(gate?, (Some(0), format!("{}\n", second.commit), true));
    Ok(())
}

#[tokio::test]
async fn absolute_diagnostics_use_the_execution_slot_for_reports_and_owner_routes()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    def.check.command = "printf '%s/mine/a.txt:1: committed failure\\n' \"$PWD\"; printf '%s/theirs/b.txt:2: committed failure\\n' \"$PWD\"; exit 1".into();
    let (code, output) = managed::check(&def, &at, "isolated-diagnostics").await?;
    assert_eq!(code, Some(1), "{output}");
    let verdict = verdict(code, &output);
    let who = Asker {
        seat: "db-dev-M".into(),
        owned: vec!["mine".into()],
        everything: false,
    };
    let report = crate::launch::fleet::check_report::answer(&def, &who, &verdict);
    assert_eq!(report.mine[0].path, "mine/a.txt");
    assert_eq!(report.others[0].path, "theirs/b.txt");
    assert_eq!(report.others[0].seat, "theirs");
    assert!(report.unowned.is_empty());
    assert!(matches!(
        crate::launch::fleet::commit_verify::judge(&def, &verdict.row, &["mine/a.txt".into()]),
        crate::launch::fleet::commit_verify::Evidence::FailsInMine(_)
    ));
    let mut command = verdict.clone();
    command.row.run_id = "fleet#command:explicit".into();
    command.row.output = format!("{}/mine/a.txt:1: shared diagnostic", def.repo);
    assert_eq!(
        baseline::diagnostics(&def, &command.row)[0].path,
        "mine/a.txt"
    );
    Ok(())
}

#[tokio::test]
async fn configured_check_keeps_process_tripwire_identity_while_using_the_isolated_slot()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    def.check.command = "touch .claude/worktrees/started; while test ! -e .claude/worktrees/finish; do sleep 0.01; done".into();
    let mut changed = def.clone();
    changed.check.command = "new configured command".into();
    let run = format!("tripwire-isolated-{}", Uuid::new_v4());
    let check = managed::check(&def, &at, &run);
    let observe = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !at.slot.join(".claude/worktrees/started").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        let notices =
            crate::launch::fleet::process_tripwire::notices(&run, &changed, Uuid::new_v4());
        std::fs::write(at.slot.join(".claude/worktrees/finish"), "finish")?;
        assert_eq!(notices.len(), 1);
        assert!(notices[0].body.contains("check.command"));
        assert!(notices[0].body.contains(at.slot.to_string_lossy().as_ref()));
        Ok::<_, anyhow::Error>(())
    };
    let (checked, observed) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(check, observe)
    })
    .await?;
    let (code, output) = checked?;
    observed?;
    assert_eq!(code, Some(0), "{output}");
    assert!(
        crate::launch::fleet::process_tripwire::notices(&run, &changed, Uuid::new_v4()).is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn unverified_isolated_diagnostics_cannot_propose_repair_work() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, def) = project(root.path()).await?;
    let slot = head(&def).await.map_err(anyhow::Error::msg)?.slot;
    let output = format!(
        "{}/tests/snapshots/missing.snap:1: missing committed snapshot",
        slot.display()
    );
    let mut result = verdict(Some(1), &output).row;
    assert_eq!(
        crate::launch::fleet::propose_tick::unowned_failures(&def, &result),
        vec!["tests/snapshots/missing.snap"]
    );
    result.exit_code = None;
    result.state_after = Some("unverified-source".into());
    assert!(crate::launch::fleet::propose_tick::unowned_failures(&def, &result).is_empty());
    Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn managed_checkout_never_runs_repository_hooks_and_ordinary_commits_keep_theirs()
-> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let first = head(&def).await.map_err(anyhow::Error::msg)?;
    for name in ["post-checkout", "post-index-change", "pre-commit"] {
        let hook = repo.join(".git/hooks").join(name);
        let marker = root.path().join(name);
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display()))?;
        std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755))?;
    }
    let (code, output) = managed::check(&def, &first, "isolated-hooks").await?;
    assert_eq!(code, Some(0), "{output}");
    assert!(!root.path().join("post-checkout").exists());
    assert!(!root.path().join("post-index-change").exists());
    assert!(!root.path().join("pre-commit").exists());
    seat_commit(&repo, "mine/a.txt", "ok again\n").await?;
    assert!(root.path().join("pre-commit").exists());
    assert!(root.path().join("post-index-change").exists());
    std::fs::remove_file(root.path().join("post-index-change"))?;
    let second = head(&def).await.map_err(anyhow::Error::msg)?;
    let (code, output) = managed::check(&def, &second, "isolated-hooks").await?;
    assert_eq!(code, Some(0), "{output}");
    assert!(!root.path().join("post-checkout").exists());
    assert!(!root.path().join("post-index-change").exists());
    assert!(
        git(&repo, &["config", "--get", "core.hooksPath"])
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn ignored_slot_files_are_preserved_when_the_next_commit_would_track_them()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    seat_commit(&repo, ".gitignore", "warm.txt\n").await?;
    let first = head(&def).await.map_err(anyhow::Error::msg)?;
    assert_eq!(
        managed::check(&def, &first, "ignored-slot").await?.0,
        Some(0)
    );
    std::fs::write(first.slot.join("warm.txt"), "cached user data")?;
    std::fs::write(repo.join("warm.txt"), "new committed file")?;
    git(&repo, &["add", "-f", "warm.txt"]).await?;
    git(&repo, &["commit", "-qm", "track formerly ignored file"]).await?;
    let second = head(&def).await.map_err(anyhow::Error::msg)?;
    let (code, output) = managed::check(&def, &second, "ignored-slot").await?;
    assert_eq!(code, None);
    assert!(output.contains("would be overwritten"), "{output}");
    assert_eq!(
        std::fs::read_to_string(first.slot.join("warm.txt"))?,
        "cached user data"
    );
    assert_eq!(
        git(&first.slot, &["rev-parse", "HEAD"]).await?,
        first.commit
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("warm.txt"))?,
        "new committed file"
    );
    Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn refused_symlink_slot_never_receives_latest_or_execution_specific_logs()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let mut target = head(&def).await.map_err(anyhow::Error::msg)?;
    target.slot = slot_for(&def.repo).ok_or_else(|| anyhow::anyhow!("no gate slot"))?;
    std::fs::create_dir_all(repo.join(".claude/worktrees"))?;
    std::fs::create_dir_all(outside.path().join(".claude/worktrees"))?;
    std::os::unix::fs::symlink(outside.path(), &target.slot)?;
    let historical = crate::launch::fleet::gate_queue::completion_log(&target, "true", "123");
    let (code, output, verified) = managed::with_context(
        "refused-slot",
        isolated::execute(
            &def.repo,
            &target,
            "refused-slot",
            "true",
            isolated::Operation::Gate {
                completion_log: Some(historical.clone()),
            },
        ),
    )
    .await?;
    assert_eq!(code, None);
    assert!(!verified);
    assert!(output.contains("not mike's isolated worktree"), "{output}");
    assert!(!historical.exists());
    assert_eq!(
        std::fs::read_dir(outside.path().join(".claude/worktrees"))?.count(),
        0
    );
    Ok(())
}
