use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{Captured, blocked, phase, record};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn run_row(persist: &PersistHandle, run: &str) -> anyhow::Result<()> {
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run.into(),
            project_path: Some("/tmp/receipts".into()),
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    Ok(())
}

fn captured(id: i64, state_after: &str) -> Captured {
    Captured {
        id,
        exit_code: Some(0),
        output: "finished\n".into(),
        state_after: state_after.into(),
    }
}

#[tokio::test]
async fn a_finished_receipt_with_a_newer_row_is_kept_against_its_own_row_and_blocks_nothing()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let run = format!("superseded-{}", Uuid::new_v4());
    run_row(&persist, &run).await?;
    let older = persist
        .fleet()
        .begin_tree_check(&run, "state-1", "make check", "seat-a")
        .await?;
    let newer = persist
        .fleet()
        .begin_tree_check(&run, "state-2", "make check", "seat-b")
        .await?;
    let repo = tempfile::tempdir()?;

    let row = record(
        &persist,
        &TaskTracker::new(),
        &run,
        &repo.path().to_string_lossy(),
        captured(older, "state-1"),
    )
    .await
    .map_err(anyhow::Error::msg)?;

    assert_eq!(row.id, older);
    assert!(row.finished_at.is_some() && row.passed());
    assert_eq!(blocked(&run), Ok(()));
    assert_eq!(phase(&run), None);
    let waiting = persist.fleet().tree_check(newer).await?;
    assert!(waiting.is_some_and(|row| row.finished_at.is_none()));
    let kept = persist.fleet().find_tree_check(&run, "state-1").await?;
    assert_eq!(kept.map(|row| row.id), Some(older));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_cancelled_receipt_that_cannot_be_recorded_does_not_block_later_checks()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let run = format!("cancelled-{}", Uuid::new_v4());
    run_row(&persist, &run).await?;
    let repo = tempfile::tempdir()?;
    let repo = repo.path().to_string_lossy().into_owned();
    let tracker = TaskTracker::new();

    let lost = record(
        &persist,
        &tracker,
        &run,
        &repo,
        captured(i64::MAX, "cancelled:unverified"),
    )
    .await;
    assert!(lost.is_err());
    assert_eq!(blocked(&run), Ok(()));

    let real = record(
        &persist,
        &tracker,
        &run,
        &repo,
        captured(i64::MAX, "state-1"),
    )
    .await;
    assert!(real.is_err());
    assert!(
        blocked(&run).is_err(),
        "a real result that is lost still blocks"
    );
    db.cleanup().await?;
    Ok(())
}
