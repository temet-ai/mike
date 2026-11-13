use super::*;

#[tokio::test]
async fn configured_check_grades_the_captured_commit_after_the_branch_advances()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    let held = crate::launch::fleet::gate_slot::queue::hold(&repo).await;
    def.check.command = "cat mine/a.txt; git rev-parse HEAD".into();
    let check = managed::check(&def, &at, "source-captured-head");
    let advance = async {
        seat_commit(&repo, "mine/a.txt", "bad\n").await?;
        std::fs::write(repo.join("theirs/new.txt"), "peer edit")?;
        drop(held);
        Ok::<_, anyhow::Error>(())
    };
    let (checked, changed) = tokio::join!(check, advance);
    let (code, output) = checked?;
    changed?;
    assert_eq!(code, Some(0), "{output}");
    assert_eq!(output, format!("ok\n{}\n", at.commit));
    assert_eq!(std::fs::read_to_string(repo.join("mine/a.txt"))?, "bad\n");
    assert!(!at.slot.join("theirs/new.txt").exists());
    Ok(())
}

#[tokio::test]
async fn configured_check_cannot_prove_source_it_changed_while_running() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    def.check.command =
        "printf changed > mine/a.txt; printf 'mine/a.txt:1: fake failure\\n'".into();
    let (code, output) = managed::check(&def, &at, "source-changed").await?;
    assert_eq!(code, None);
    assert!(output.contains("unverified after execution"), "{output}");
    assert!(
        output.contains("Original command exit: Some(0)"),
        "{output}"
    );
    assert!(output.contains("put back"), "{output}");
    assert!(output.contains("mine/a.txt"), "{output}");
    assert_eq!(std::fs::read_to_string(at.slot.join("mine/a.txt"))?, "ok\n");
    assert_eq!(std::fs::read_to_string(repo.join("mine/a.txt"))?, "ok\n");
    Ok(())
}

#[tokio::test]
async fn verified_state_binds_effective_command_and_rejects_legacy_content_hash()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, mut def) = project(root.path()).await?;
    let first = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    let raw = crate::launch::fleet::watch::git(&def.repo, &["ls-tree", "-r", "-z", "HEAD"], None)
        .await
        .ok_or_else(|| anyhow::anyhow!("missing committed tree"))?;
    let legacy = crate::launch::fleet::check::state_of(
        &crate::launch::fleet::check::parse_tree(&raw),
        &def.check.covered(),
    )
    .ok_or_else(|| anyhow::anyhow!("no state"))?;
    assert_ne!(first, legacy);
    def.check.command = "false".into();
    let changed = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(first, changed);
    Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn committed_executable_mode_changes_invalidate_the_check_state() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    let file = repo.join("mine/a.txt");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
    git(&repo, &["add", "mine/a.txt"]).await?;
    git(&repo, &["commit", "-qm", "make executable"]).await?;
    def.check.command = "test -x mine/a.txt".into();
    let before = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
    git(&repo, &["add", "mine/a.txt"]).await?;
    git(&repo, &["commit", "-qm", "remove executable mode"]).await?;
    let after = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(before, after);
    Ok(())
}

#[tokio::test]
async fn unverified_automatic_check_never_records_proof_or_blames_a_seat() -> anyhow::Result<()> {
    managed::with_private_root(async {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        persist
            .launch()
            .ensure_session_exists(&Uuid::from_u128(SEAT_SESSION).to_string())
            .await?;
        let root = tempfile::tempdir()?;
        let (_, mut def) = project(root.path()).await?;
        def.check.command =
            "printf changed > mine/a.txt; printf 'mine/a.txt:1: fake failure\\n'".into();
        let tracker = TaskTracker::new();
        let run = format!("source-unverified-{}", Uuid::new_v4());
        let fleet = managed::running_fleet(&persist, &def, &run).await?;
        seat_m(&persist, &run).await?;
        let runner = Runner {
            persist: &persist,
            tracker: &tracker,
            run: &run,
            fleet,
        };
        let row = queued_check(&runner, &def, &tracker, &persist, &run).await?;
        assert_eq!(row.exit_code, None);
        assert_ne!(row.state_after.as_deref(), Some(row.state.as_str()));
        assert!(
            persist
                .fleet()
                .find_tree_check(&run, &row.state)
                .await?
                .is_none()
        );
        assert!(results(&persist).await?.is_empty());
        assert!(inbox(&persist).await?.is_empty());
        db.cleanup().await?;
        Ok(())
    })
    .await?
}
