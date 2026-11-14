use super::super::{baseline, target};
use super::{git, managed, project, seat_commit};
use crate::launch::fleet::check::CheckRequest;
use uuid::Uuid;

#[tokio::test]
async fn baseline_preserves_every_shared_edit_and_reuses_its_build_directory() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let first = git(&repo, &["rev-parse", "HEAD"]).await?;
    seat_commit(&repo, "mine/a.txt", "bad\n").await?;
    std::fs::write(repo.join("mine/a.txt"), "staged peer edit\n")?;
    git(&repo, &["add", "mine/a.txt"]).await?;
    std::fs::write(repo.join("mine/a.txt"), "unstaged peer edit\n")?;
    std::fs::write(repo.join("theirs/new.txt"), "untracked peer edit\n")?;
    let before = git(&repo, &["status", "--porcelain"]).await?;
    let index = git(&repo, &["show", ":mine/a.txt"]).await?;
    let head = git(&repo, &["rev-parse", "HEAD"]).await?;
    let at = target(&def, Some(&first))
        .await
        .map_err(anyhow::Error::msg)?;
    let (code, output) = managed::check(&def, &at, "fleet-baseline#baseline:first").await?;
    assert_eq!(code, Some(0), "{output}");
    std::fs::create_dir_all(at.slot.join("target"))?;
    std::fs::write(at.slot.join("target/warm"), "cached")?;
    let common = git(&repo, &["rev-parse", "--git-common-dir"]).await?;
    let exclude = repo.join(common).join("info/exclude");
    let mut ignored = std::fs::read_to_string(&exclude)?;
    ignored.push_str("target/\n");
    std::fs::write(exclude, ignored)?;
    let newest = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    let (code, output) = managed::check(&def, &newest, "fleet-baseline#baseline:newest").await?;
    assert_eq!(code, Some(1), "{output}");
    assert_eq!(newest.slot, at.slot);
    assert_eq!(
        std::fs::read_to_string(at.slot.join("target/warm"))?,
        "cached"
    );
    assert_eq!(git(&repo, &["status", "--porcelain"]).await?, before);
    assert_eq!(git(&repo, &["show", ":mine/a.txt"]).await?, index);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).await?, head);
    assert_eq!(
        std::fs::read_to_string(repo.join("mine/a.txt"))?,
        "unstaged peer edit\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("theirs/new.txt"))?,
        "untracked peer edit\n"
    );
    Ok(())
}

#[tokio::test]
async fn dirty_baseline_is_refused_without_removing_any_files() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, def) = project(root.path()).await?;
    let at = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(managed::check(&def, &at, "baseline").await?.0, Some(0));
    std::fs::write(at.slot.join("mine/a.txt"), "do not erase")?;
    std::fs::write(at.slot.join("unknown.txt"), "keep this too")?;
    let (code, output) = managed::check(&def, &at, "baseline").await?;
    assert_eq!(code, None);
    assert!(output.contains("has changes"), "{output}");
    assert_eq!(
        std::fs::read_to_string(at.slot.join("mine/a.txt"))?,
        "do not erase"
    );
    assert_eq!(
        std::fs::read_to_string(at.slot.join("unknown.txt"))?,
        "keep this too"
    );
    Ok(())
}

#[tokio::test]
async fn baseline_requests_resolve_once_and_cannot_reuse_shared_or_other_commit_verdicts()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let request = CheckRequest {
        session: Uuid::new_v4(),
        wait_seconds: None,
        status: false,
        test: None,
        command: None,
        commit: Some("HEAD".into()),
        ruling: None,
        withdraw: None,
    };
    let (pinned, first) = baseline::request(&def, "fleet#test:one", &request)
        .await
        .map_err(anyhow::Error::msg)?;
    let head = git(&repo, &["rev-parse", "HEAD"]).await?;
    assert_eq!(pinned.commit.as_deref(), Some(head.as_str()));
    assert!(baseline::is_baseline(&first));
    seat_commit(&repo, "notes.md", "outside check paths").await?;
    let (_, second) = baseline::request(&def, "fleet#test:one", &request)
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(first, second);
    let (_, shared) = baseline::request(
        &def,
        "fleet#test:one",
        &CheckRequest {
            commit: None,
            ..request
        },
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(shared, "fleet#test:one");
    assert!(!baseline::is_baseline(&shared));
    assert_eq!(super::super::base_run(&first), "fleet");
    Ok(())
}

#[tokio::test]
async fn concurrent_historical_tests_keep_their_own_commit_through_execution() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    let first = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    seat_commit(&repo, "mine/a.txt", "new\n").await?;
    let second = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    def.check.command = "cat mine/a.txt; sleep 0.1; cat mine/a.txt".into();
    let (before, after) = tokio::join!(
        managed::check(&def, &first, "fleet-a"),
        managed::check(&def, &second, "fleet-b#test:one")
    );
    assert_eq!(before?, (Some(0), "ok\nok\n".into()));
    assert_eq!(after?, (Some(0), "new\nnew\n".into()));
    Ok(())
}

#[tokio::test]
async fn a_check_that_changes_the_baseline_cannot_claim_a_passing_commit() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, mut def) = project(root.path()).await?;
    let at = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    def.check.command = "echo replaced > mine/a.txt".into();
    let (code, output) = managed::check(&def, &at, "baseline").await?;
    assert_eq!(code, None);
    assert!(output.contains("not valid"), "{output}");
    Ok(())
}

#[test]
fn historical_failures_are_labeled_without_claiming_current_owners_were_notified() {
    use crate::launch::fleet::check::Asker;
    use crate::launch::fleet::check_report::{answer, text};
    use crate::launch::fleet::tests::{complete, parsed};
    let def = parsed(&complete());
    let mut verdict = super::verdict(Some(1), "old test failed");
    verdict.row.run_id = "fleet#baseline:old".into();
    let who = Asker {
        seat: "worker".into(),
        owned: vec![],
        everything: false,
    };
    let report = answer(&def, &who, &verdict);
    assert!(report.baseline);
    assert!(text(&report).starts_with("Baseline:"));
    assert!(!text(&report).contains("has been told"));
}

#[tokio::test]
async fn a_baseline_cannot_adopt_an_existing_directory_or_attached_branch() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let at = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::create_dir_all(&at.slot)?;
    std::fs::write(at.slot.join("keep"), "user data")?;
    let (code, output) = managed::check(&def, &at, "baseline").await?;
    assert_eq!(code, None);
    assert!(output.contains("not mike's isolated worktree"), "{output}");
    assert_eq!(std::fs::read_to_string(at.slot.join("keep"))?, "user data");
    std::fs::remove_file(at.slot.join("keep"))?;
    std::fs::remove_dir(&at.slot)?;
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "someone",
            &at.slot.to_string_lossy(),
        ],
    )
    .await?;
    let (code, output) = managed::check(&def, &at, "baseline").await?;
    assert_eq!(code, None);
    assert!(output.contains("not a detached worktree"), "{output}");
    assert_eq!(
        git(&at.slot, &["branch", "--show-current"]).await?,
        "someone"
    );
    Ok(())
}

#[test]
fn baseline_setup_and_execution_errors_are_not_reusable_verdicts() {
    assert_ne!(
        baseline::state_after("fleet#baseline:sha", "state", None),
        "state"
    );
    assert_eq!(
        baseline::state_after("fleet#baseline:sha", "state", Some(0)),
        "state"
    );
    assert_eq!(
        baseline::state_after("fleet#baseline:sha", "state", Some(1)),
        "state"
    );
    assert_ne!(baseline::state_after("fleet", "state", None), "state");
}
