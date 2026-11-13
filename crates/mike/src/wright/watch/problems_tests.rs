use crate::store::persistence::web_repo::WebPersist as _;
use chrono::Utc;
use serde_json::json;

use super::fixtures_tests::Db;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::{ProblemFilter, RecordedRun};
use crate::wright::{CheckOutcome, CheckRun, Finding};

const CHECK: &str = "request_failures";

fn finding(resource: &str, n: i64) -> Finding {
    Finding {
        scope: "capture".to_owned(),
        resource: resource.to_owned(),
        evidence: json!({"failed_requests": n}),
    }
}

async fn record(handle: &PersistHandle, outcome: &CheckOutcome) -> anyhow::Result<RecordedRun> {
    Ok(handle
        .web()
        .record_wright_run(&CheckRun {
            check: CHECK,
            source: "capture_db",
            cadence: "event",
            started_at: Utc::now(),
            outcome,
            machine: None,
        })
        .await?)
}

fn all(limit: i64, offset: i64) -> ProblemFilter {
    ProblemFilter {
        limit,
        offset,
        ..ProblemFilter::default()
    }
}

async fn counts(db: &Db) -> anyhow::Result<(i64, i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT (SELECT count(*) FROM wright.problems),
                (SELECT count(*) FROM wright.episodes),
                (SELECT count(*) FROM wright.episodes WHERE closed_at IS NULL)",
    )
    .fetch_one(db.handle.pool())
    .await?)
}

#[tokio::test]
async fn repeated_scans_keep_one_problem_and_one_open_episode() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let first = record(
        &db.handle,
        &CheckOutcome::Complete(vec![finding("stream aborted", 5)]),
    )
    .await?;
    assert_eq!((first.opened, first.still_open), (1, 0));
    for n in 6..9 {
        let again = record(
            &db.handle,
            &CheckOutcome::Complete(vec![finding("stream aborted", n)]),
        )
        .await?;
        assert_eq!((again.opened, again.still_open), (0, 1));
    }
    assert_eq!(counts(&db).await?, (1, 1, 1));
    let page = db.handle.web().list_wright_problems(&all(10, 0)).await?;
    assert_eq!(
        page.problems[0].latest_evidence,
        Some(json!({"failed_requests": 8}))
    );
    db.cleanup().await
}

#[tokio::test]
async fn competing_workers_open_one_problem_per_check_scope_and_resource() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let other = PersistHandle::connect(&db.db.database_url).await?;
    let outcome = CheckOutcome::Complete(vec![
        finding("stream aborted", 5),
        finding("stream aborted", 5),
    ]);
    let runs = futures::future::try_join_all((0..8).map(|i| {
        let handle = if i % 2 == 0 { &db.handle } else { &other };
        record(handle, &outcome)
    }))
    .await?;
    assert_eq!(runs.iter().map(|r| r.opened).sum::<i64>(), 1);
    assert_eq!(counts(&db).await?, (1, 1, 1));
    db.cleanup().await
}

#[tokio::test]
async fn a_recurrence_after_a_clean_run_is_a_new_episode_of_the_same_problem() -> anyhow::Result<()>
{
    let db = Db::boot().await?;
    record(
        &db.handle,
        &CheckOutcome::Complete(vec![finding("stream aborted", 5)]),
    )
    .await?;
    let clean = record(&db.handle, &CheckOutcome::Complete(Vec::new())).await?;
    assert_eq!(clean.healthy, 1);
    let page = db.handle.web().list_wright_problems(&all(10, 0)).await?;
    assert_eq!(page.problems[0].state, "healthy");

    let again = record(
        &db.handle,
        &CheckOutcome::Complete(vec![finding("stream aborted", 7)]),
    )
    .await?;
    assert_eq!((again.opened, again.recurred), (0, 1));
    assert_eq!(counts(&db).await?, (1, 2, 1));
    let page = db.handle.web().list_wright_problems(&all(10, 0)).await?;
    assert_eq!(page.problems[0].state, "open");
    assert_eq!(page.problems[0].episodes, 2);
    let episodes = db
        .handle
        .web()
        .wright_episodes(page.problems[0].id, 10, 0)
        .await?;
    assert!(episodes[0].closed_at.is_none());
    assert_eq!(episodes[1].closed_by_run, Some(clean.run_id));
    db.cleanup().await
}

#[tokio::test]
async fn an_unknown_run_never_marks_a_problem_healthy() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    record(
        &db.handle,
        &CheckOutcome::Complete(vec![finding("stream aborted", 5)]),
    )
    .await?;
    let unknown = record(
        &db.handle,
        &CheckOutcome::Unknown(
            "request_failures: query failed: canceling statement due to statement timeout"
                .to_owned(),
        ),
    )
    .await?;
    assert_eq!(unknown.healthy, 0);
    let page = db.handle.web().list_wright_problems(&all(10, 0)).await?;
    assert_eq!(page.problems[0].state, "open");
    assert_eq!(counts(&db).await?, (1, 1, 1));
    let runs = db.handle.web().latest_wright_runs().await?;
    assert_eq!(runs[0].outcome, "unknown");
    assert!(
        runs[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("statement timeout"))
    );
    db.cleanup().await
}

#[tokio::test]
async fn problems_page_through_every_row_with_a_stable_order() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let findings: Vec<Finding> = (0..7).map(|n| finding(&format!("shape {n}"), n)).collect();
    record(&db.handle, &CheckOutcome::Complete(findings)).await?;
    let mut seen = Vec::new();
    let mut offset = 0;
    loop {
        let page = db
            .handle
            .web()
            .list_wright_problems(&all(3, offset))
            .await?;
        assert_eq!(page.total, 7);
        if page.problems.is_empty() {
            break;
        }
        offset += i64::try_from(page.problems.len())?;
        seen.extend(page.problems.into_iter().map(|p| p.resource));
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 7);
    let open = ProblemFilter {
        state: Some("healthy".to_owned()),
        ..all(10, 0)
    };
    assert_eq!(db.handle.web().list_wright_problems(&open).await?.total, 0);
    db.cleanup().await
}
