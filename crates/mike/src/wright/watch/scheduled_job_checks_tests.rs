use crate::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use super::fixtures_tests::Db;

async fn run(
    db: &Db,
    job: &str,
    machine: Uuid,
    ago: &str,
    result_kind: &str,
    exit_code: i32,
    failures: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO jobs.runs (job_name, machine_uuid, trigger, started_at, finished_at, duration_ms,
                                result_kind, exit_code, output_tail, consecutive_failures)
         VALUES ($1, $2, 'schedule', now() - $3::interval, now() - $3::interval, 5, $4, $5, '', $6)",
    )
    .bind(job)
    .bind(machine)
    .bind(ago)
    .bind(result_kind)
    .bind(exit_code)
    .bind(failures)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn scheduled_job_failures_leased_job_succeeded_elsewhere_reports_nothing()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let stale = Uuid::new_v4();
    let holder = Uuid::new_v4();
    run(&db, "decisions_loop", stale, "50 hours", "exited", 1, 3).await?;
    run(&db, "decisions_loop", holder, "20 hours", "succeeded", 0, 0).await?;
    run(&db, "learning_promote", stale, "1 hour", "succeeded", 0, 0).await?;
    let findings = db.findings("scheduled_job_failures").await?;
    assert!(
        findings.iter().all(|f| !f.resource.starts_with("job:decisions_loop")),
        "{findings:?}"
    );
    db.cleanup().await
}

#[tokio::test]
async fn scheduled_job_failures_leased_job_failing_reports_it_once_without_machine()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    run(&db, "memory_review", first, "30 hours", "exited", 1, 2).await?;
    run(&db, "memory_review", second, "2 hours", "exited", 1, 3).await?;
    let findings = db.findings("scheduled_job_failures").await?;
    let leased: Vec<_> = findings
        .iter()
        .filter(|f| f.resource.starts_with("job:memory_review"))
        .collect();
    assert_eq!(leased.len(), 1, "{findings:?}");
    assert_eq!(leased[0].resource, "job:memory_review");
    assert_eq!(leased[0].evidence["consecutive_failures"], 3);
    db.cleanup().await
}

#[tokio::test]
async fn scheduled_job_failures_per_machine_job_failing_on_one_machine_reports_that_machine()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let failing = Uuid::new_v4();
    let fine = Uuid::new_v4();
    run(&db, "learning_promote", failing, "2 hours", "exited", 1, 2).await?;
    run(&db, "learning_promote", fine, "1 hour", "succeeded", 0, 0).await?;
    let findings = db.findings("scheduled_job_failures").await?;
    let resources: Vec<_> = findings
        .iter()
        .filter(|f| f.resource.starts_with("job:learning_promote"))
        .map(|f| f.resource.clone())
        .collect();
    assert_eq!(resources, vec![format!("job:learning_promote@{failing}")]);
    db.cleanup().await
}

#[tokio::test]
async fn scheduled_job_failures_leased_jobs_param_matches_the_scheduler() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let Some(row) = db.handle.web().wright_check("scheduled_job_failures").await? else {
        anyhow::bail!("the seed must hold scheduled_job_failures");
    };
    let mut stored: Vec<String> = serde_json::from_value(row.params["leased_jobs"].clone())?;
    stored.sort_unstable();
    assert_eq!(stored, crate::scheduled_jobs::leased_job_names());
    db.cleanup().await
}
