use chrono::{DateTime, Duration, TimeZone, Utc};
use uuid::Uuid;

use super::{JobRunRow, LatestJobRuns};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

async fn connect() -> anyhow::Result<(TestDbHandle, PersistHandle)> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok((db, handle))
}

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, 0)
        .single()
        .unwrap_or_default()
}

fn run(job: &str, machine: Option<Uuid>, trigger: &str, started: DateTime<Utc>) -> JobRunRow {
    JobRunRow {
        job_name: job.to_owned(),
        machine_uuid: machine,
        trigger: trigger.to_owned(),
        started_at: started,
        finished_at: started + Duration::seconds(30),
        duration_ms: 30_000,
        result_kind: "exited".to_owned(),
        exit_code: Some(2),
        result_detail: None,
        output_tail: "line one\nline two".to_owned(),
        consecutive_failures: 2,
        retry_after: Some(started + Duration::hours(1)),
    }
}

#[tokio::test]
async fn latest_runs_for_a_job_that_never_ran_is_empty() -> anyhow::Result<()> {
    let (db, handle) = connect().await?;
    let found = handle.latest_scheduled_job_runs("never_ran", None).await?;
    assert_eq!(found, LatestJobRuns::default());
    db.cleanup().await
}

#[tokio::test]
async fn latest_runs_returns_the_newest_row_and_the_newest_scheduled_start() -> anyhow::Result<()> {
    let (db, handle) = connect().await?;
    let scheduled = run("promote", None, "schedule", at(21, 30));
    let manual = run("promote", None, "manual", at(22, 5));
    let older = run("promote", None, "schedule", at(20, 30));
    for row in [&older, &scheduled, &manual] {
        handle.record_scheduled_job_run(row).await?;
    }
    let found = handle.latest_scheduled_job_runs("promote", None).await?;
    assert_eq!(found.latest, Some(manual));
    assert_eq!(found.last_scheduled_start, Some(at(21, 30)));
    db.cleanup().await
}

#[tokio::test]
async fn latest_runs_for_a_machine_ignore_other_machines() -> anyhow::Result<()> {
    let (db, handle) = connect().await?;
    let mine = Uuid::new_v4();
    let theirs = Uuid::new_v4();
    let my_run = run("sweep", Some(mine), "schedule", at(6, 0));
    let their_run = run("sweep", Some(theirs), "schedule", at(12, 0));
    handle.record_scheduled_job_run(&my_run).await?;
    handle.record_scheduled_job_run(&their_run).await?;
    let for_me = handle
        .latest_scheduled_job_runs("sweep", Some(mine))
        .await?;
    assert_eq!(for_me.latest, Some(my_run));
    assert_eq!(for_me.last_scheduled_start, Some(at(6, 0)));
    let any = handle.latest_scheduled_job_runs("sweep", None).await?;
    assert_eq!(any.latest, Some(their_run));
    db.cleanup().await
}

#[tokio::test]
async fn record_run_with_an_exit_code_on_an_unrunnable_result_is_refused() -> anyhow::Result<()> {
    let (db, handle) = connect().await?;
    let mut bad = run("audit", None, "schedule", at(1, 0));
    bad.result_kind = "unrunnable".to_owned();
    let refused = handle.record_scheduled_job_run(&bad).await;
    assert!(refused.is_err(), "{refused:?}");
    db.cleanup().await
}
