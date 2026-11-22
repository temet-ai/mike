use super::fixtures_tests::Db;
use crate::wright::Finding;

const POLL: &str =
    "SELECT id FROM wright.problems WHERE check_name = $1 AND scope = $2 AND resource = $3";

async fn boot(polls: usize) -> anyhow::Result<Db> {
    let db = Db::boot().await?;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(db.handle.pool())
        .await?;
    sqlx::query(
        "UPDATE wright.checks
            SET params = params || jsonb_build_object('roles', jsonb_build_array(current_user))
          WHERE name = 'idle_polls'",
    )
    .execute(db.handle.pool())
    .await?;
    poll(&db, polls).await?;
    Ok(db)
}

async fn poll(db: &Db, times: usize) -> anyhow::Result<()> {
    for _ in 0..times {
        sqlx::query(POLL)
            .bind("none")
            .bind("none")
            .bind("none")
            .fetch_optional(db.handle.pool())
            .await?;
    }
    Ok(())
}

async fn snapshot(db: &Db, ago: &str, calls_since: i64) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "WITH st AS (
             SELECT s.queryid, sum(s.calls) AS calls, sum(s.rows) AS rows,
                    min(s.stats_since) AS since
               FROM pg_stat_statements s
              WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
                AND s.toplevel AND s.query = $1
              GROUP BY s.queryid
         )
         INSERT INTO wright.statement_counters (queryid, taken_at, calls, rows, total_ms, stats_since)
         SELECT st.queryid, now() - $2::interval, st.calls - $3, st.rows, 0, st.since
           FROM st
         RETURNING queryid::text",
    )
    .bind(POLL)
    .bind(ago)
    .bind(calls_since)
    .fetch_one(db.handle.pool())
    .await?)
}

async fn reported(db: &Db, queryid: &str) -> anyhow::Result<Vec<Finding>> {
    Ok(db
        .findings("idle_polls")
        .await?
        .into_iter()
        .filter(|f| f.evidence["queryid"] == queryid)
        .collect())
}

async fn snapshots(db: &Db, queryid: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM wright.statement_counters WHERE queryid::text = $1",
    )
    .bind(queryid)
    .fetch_one(db.handle.pool())
    .await?)
}

async fn poll_queryid(db: &Db) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT queryid::text FROM pg_stat_statements
          WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
            AND toplevel AND query = $1
          LIMIT 1",
    )
    .bind(POLL)
    .fetch_one(db.handle.pool())
    .await?)
}

#[tokio::test]
async fn idle_polls_on_a_never_flagged_statement_polling_since_its_snapshot_reports_it()
-> anyhow::Result<()> {
    let db = boot(600).await?;
    let queryid = snapshot(&db, "40 minutes", 600).await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].evidence["window_calls"], 600);
    let rate = found[0].evidence["calls_per_hour"]
        .as_f64()
        .unwrap_or_default();
    assert!((899.0..=900.0).contains(&rate), "{rate}");
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_on_a_statement_quiet_since_its_snapshot_reports_nothing() -> anyhow::Result<()>
{
    let db = boot(600).await?;
    let queryid = snapshot(&db, "40 minutes", 0).await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_with_only_a_young_snapshot_reports_nothing() -> anyhow::Result<()> {
    let db = boot(600).await?;
    let queryid = snapshot(&db, "10 minutes", 600).await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_on_a_statement_first_seen_now_reports_nothing_and_saves_its_counters()
-> anyhow::Result<()> {
    let db = boot(600).await?;
    assert_eq!(db.findings("idle_polls").await?, Vec::new());
    let queryid = poll_queryid(&db).await?;
    assert_eq!(snapshots(&db, &queryid).await?, 1);
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_after_a_counter_reset_reports_nothing_and_starts_again() -> anyhow::Result<()> {
    let db = boot(10).await?;
    let queryid = snapshot(&db, "40 minutes", -5000).await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    assert_eq!(snapshots(&db, &queryid).await?, 1);
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_keeps_at_most_two_snapshots_of_a_statement() -> anyhow::Result<()> {
    let db = boot(600).await?;
    snapshot(&db, "95 minutes", 600).await?;
    snapshot(&db, "65 minutes", 400).await?;
    let queryid = snapshot(&db, "35 minutes", 200).await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].evidence["window_calls"], 200);
    assert_eq!(snapshots(&db, &queryid).await?, 2);
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_a_snapshot_newer_than_the_window_waits_and_is_not_replaced()
-> anyhow::Result<()> {
    let db = boot(600).await?;
    snapshot(&db, "45 minutes", 600).await?;
    let queryid = snapshot(&db, "5 minutes", 50).await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].evidence["window_calls"], 600);
    assert_eq!(snapshots(&db, &queryid).await?, 2);
    db.cleanup().await
}
