//! The rest of fleet_shutdown's tests.

use super::*;

#[tokio::test]
async fn absent_persisted_pid_without_local_origin_never_completes_fleet() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let mut launch = NewLaunch::new("none", "/foreign/checkout");
    launch.run_id = Some(f.run.run_id.clone());
    launch.session_uuid = Some(Uuid::new_v4());
    let id = f.persist.launch().insert_launch(&launch).await?;
    f.persist
        .launch()
        .record_launch_opened(
            id,
            &LaunchSurfaceRefs {
                pid: Some(i32::MAX),
                surface_ref: Some("foreign-pane".into()),
                ..LaunchSurfaceRefs::default()
            },
        )
        .await?;
    assert!(!shutdown::reconcile(&f.state, &f.persist, &f.row).await?);
    assert_eq!(f.refresh().await?.state, "running");
    let stopped = shutdown::stop(&f.state, &f.persist, &f.row).await?;
    assert_eq!(stopped.state, "stopping");
    assert_eq!(stopped.seats[0].process_exited, None);
    let unresolved = f
        .persist
        .fleet()
        .unsettled_fleet_launches(&f.run.run_id)
        .await?;
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].close_process_exited, None);
    assert!(f.refresh().await?.finished_at.is_none());
    f.cleanup().await
}

async fn owned_root_fixture(
    f: &Fixture,
    id: i64,
    session: Uuid,
) -> anyhow::Result<tokio::process::Child> {
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_mike"))
        .args(["fleet", "background-hook"])
        .env("MIKE_FLEET_BACKGROUND_COMMANDS", r#"["slow"]"#)
        .env_remove("CLAUDE_CODE_DISABLE_BACKGROUND_TASKS")
        .env("MIKE_SESSION_ID", session.to_string())
        .env("MIKE_RUN_ID", &f.run.run_id)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow::anyhow!("fixture child has no PID"))?;
    tokio::time::timeout(FIXTURE_CHILD_LIMIT, async {
        loop {
            if mike::launch::cli::test_support::remember_owned_root_fixture(
                id,
                &f.run.run_id,
                session,
                pid,
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(child)
}

#[tokio::test]
async fn same_proxy_witness_proves_natural_exit_of_its_owned_fixture_root() -> anyhow::Result<()> {
    let Some(mut f) = Fixture::starting().await? else {
        return Ok(());
    };
    f.running().await?;
    let session = Uuid::new_v4();
    let mut launch = NewLaunch::new("herdr", f.repo.path().display().to_string());
    launch.run_id = Some(f.run.run_id.clone());
    launch.session_uuid = Some(session);
    let id = f.persist.launch().insert_launch(&launch).await?;
    let mut child = owned_root_fixture(&f, id, session).await?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow::anyhow!("fixture child has no PID"))?;
    f.persist
        .launch()
        .record_launch_opened(
            id,
            &LaunchSurfaceRefs {
                pid: Some(i32::try_from(pid)?),
                surface_ref: Some("w1:p1".into()),
                ..LaunchSurfaceRefs::default()
            },
        )
        .await?;
    drop(child.stdin.take());
    tokio::time::timeout(FIXTURE_CHILD_LIMIT, child.wait()).await??;
    assert!(shutdown::reconcile(&f.state, &f.persist, &f.row).await?);
    assert_eq!(f.refresh().await?.state, "done");
    assert!(
        f.persist
            .fleet()
            .unsettled_fleet_launches(&f.run.run_id)
            .await?
            .is_empty()
    );
    f.cleanup().await
}
