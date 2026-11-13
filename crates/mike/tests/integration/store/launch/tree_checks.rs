use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch::NewRun;
use mike::store::persistence::launch_repo::LaunchPersist as _;

use crate::test_db::TestDb;

#[tokio::test]
async fn a_check_is_recorded_per_state_with_who_asked_and_who_was_told() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    handle
        .launch()
        .upsert_run(&NewRun {
            run_id: "fleet-checks-1".to_owned(),
            project_path: Some("/tmp/tree-checks".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;

    let id = handle
        .fleet()
        .begin_tree_check("fleet-checks-1", "state-a", "make check", "db-dev-T1")
        .await?;
    assert_eq!(
        handle
            .fleet()
            .find_tree_check("fleet-checks-1", "state-a")
            .await?,
        None,
        "a check still running is not served as a result"
    );
    let running = handle.fleet().latest_tree_check("fleet-checks-1").await?;
    assert_eq!(running.as_ref().map(|r| r.id), Some(id));
    assert!(running.is_some_and(|r| r.finished_at.is_none() && !r.passed()));

    handle
        .fleet()
        .finish_tree_check(id, Some(1), "error\n  --> src/a.rs:3:1\n", Some("state-a"))
        .await?;
    handle.fleet().add_tree_check_asker(id, "db-dev-T2").await?;
    handle.fleet().add_tree_check_asker(id, "db-dev-T2").await?;
    assert!(handle.fleet().add_tree_check_told(id, "db-dev-T1").await?);
    assert!(
        !handle.fleet().add_tree_check_told(id, "db-dev-T1").await?,
        "a seat is told once per check"
    );

    let row = handle
        .fleet()
        .find_tree_check("fleet-checks-1", "state-a")
        .await?
        .ok_or_else(|| anyhow::anyhow!("the finished check should be found by its state"))?;
    assert_eq!(row.exit_code, Some(1));
    assert!(!row.passed());
    assert_eq!(row.output, "error\n  --> src/a.rs:3:1\n");
    assert_eq!(row.askers, vec!["db-dev-T1", "db-dev-T2"]);
    assert_eq!(row.told, vec!["db-dev-T1"]);
    assert_eq!(
        handle
            .fleet()
            .find_tree_check("fleet-checks-1", "state-b")
            .await?,
        None
    );

    let moved = handle
        .fleet()
        .begin_tree_check("fleet-checks-1", "state-c", "make check", "db-dev-T1")
        .await?;
    handle
        .fleet()
        .finish_tree_check(moved, Some(0), "ok", Some("state-d"))
        .await?;
    assert_eq!(
        handle
            .fleet()
            .find_tree_check("fleet-checks-1", "state-c")
            .await?,
        None,
        "a check the tree moved under is not served for the state it started at"
    );
    assert!(
        handle
            .fleet()
            .latest_tree_check("fleet-checks-1")
            .await?
            .is_some_and(|r| r.id == moved && r.moved())
    );
    db.cleanup().await?;
    Ok(())
}
