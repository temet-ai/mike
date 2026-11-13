use chrono::Utc;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch::{NewBackendValidation, NewLaunch, NewRun};

use crate::test_db::TestDb;

#[tokio::test]
async fn run_upsert_keeps_recorded_tab_and_close_is_idempotent() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let run_id = format!("run-a-{}", Uuid::new_v4());

    handle
        .launch()
        .upsert_run(&NewRun {
            run_id: run_id.clone(),
            project_path: Some("/tmp/run-a".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: Some("ws-1".to_owned()),
            tab_ref: Some("tab-1".to_owned()),
        })
        .await?;
    handle
        .launch()
        .upsert_run(&NewRun {
            run_id: run_id.clone(),
            project_path: None,
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let run = handle
        .launch()
        .find_run(&run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run-a missing"))?;
    assert_eq!(run.tab_ref.as_deref(), Some("tab-1"));
    assert_eq!(run.project_path.as_deref(), Some("/tmp/run-a"));
    assert!(run.closed_at.is_none());

    let mut new = NewLaunch::new("herdr", "/tmp/run-a");
    new.run_id = Some(run_id.clone());
    handle.launch().insert_launch(&new).await?;
    let mut dangling = NewLaunch::new("herdr", "/tmp/run-a");
    dangling.run_id = Some("run-never-recorded".to_owned());
    assert!(
        handle.launch().insert_launch(&dangling).await.is_err(),
        "a launch must not reference a run that was never recorded"
    );

    handle.launch().close_run(&run_id).await?;
    let closed_at = handle
        .launch()
        .find_run(&run_id)
        .await?
        .and_then(|r| r.closed_at)
        .ok_or_else(|| anyhow::anyhow!("run-a not closed"))?;
    handle.launch().close_run(&run_id).await?;
    let again = handle
        .launch()
        .find_run(&run_id)
        .await?
        .and_then(|r| r.closed_at);
    assert_eq!(
        again,
        Some(closed_at),
        "a repeated close keeps the first close time"
    );
    assert!(handle.launch().close_run("run-unknown").await.is_err());

    Ok(())
}

#[tokio::test]
async fn latest_backend_validation_returns_newest_with_full_report() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    assert!(
        handle
            .launch()
            .latest_backend_validation("herdr")
            .await?
            .is_none()
    );

    let checks = serde_json::json!([
        {"name": "binary", "passed": true, "detail": "herdr 0.9.2", "remedy": null},
        {"name": "server", "passed": false, "detail": "socket missing", "remedy": "herdr serve"}
    ]);
    let earlier = Utc::now() - chrono::Duration::minutes(5);
    let base = NewBackendValidation {
        backend: "herdr".to_owned(),
        passed: false,
        version: Some("0.9.2".to_owned()),
        protocol: Some("22".to_owned()),
        checks: checks.clone(),
        started_at: earlier,
        finished_at: earlier,
        source: "setup".to_owned(),
    };
    handle.launch().insert_backend_validation(&base).await?;
    let now = Utc::now();
    let newest_id = handle
        .launch()
        .insert_backend_validation(&NewBackendValidation {
            passed: true,
            started_at: now,
            finished_at: now,
            source: "doctor".to_owned(),
            ..base.clone()
        })
        .await?;

    let latest = handle
        .launch()
        .latest_backend_validation("herdr")
        .await?
        .ok_or_else(|| anyhow::anyhow!("no validation stored"))?;
    assert_eq!(latest.id, newest_id);
    assert!(latest.passed);
    assert_eq!(latest.source, "doctor");
    assert_eq!(latest.checks, checks);
    assert!(
        handle
            .launch()
            .latest_backend_validation("tmux")
            .await?
            .is_none()
    );

    let bad_source = NewBackendValidation {
        source: "cron".to_owned(),
        ..base
    };
    assert!(
        handle
            .launch()
            .insert_backend_validation(&bad_source)
            .await
            .is_err(),
        "a source outside the CHECK set must be refused"
    );

    handle_db.cleanup().await?;
    Ok(())
}
