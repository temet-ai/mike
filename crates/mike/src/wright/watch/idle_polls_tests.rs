use crate::store::persistence::web_repo::WebPersist as _;
use chrono::Utc;

use super::fixtures_tests::Db;
use crate::wright::{CheckOutcome, CheckRun, Finding};

const POLL: &str =
    "SELECT id FROM wright.problems WHERE check_name = $1 AND scope = $2 AND resource = $3";

struct Past<'a> {
    closed: bool,
    seen_ago: &'a str,
    since_seen: i64,
    base_ago: &'a str,
    since_base: i64,
    prev: Option<(&'a str, i64)>,
    carried_rate: Option<f64>,
}

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
    for _ in 0..polls {
        sqlx::query(POLL)
            .bind("none")
            .bind("none")
            .bind("none")
            .fetch_optional(db.handle.pool())
            .await?;
    }
    Ok(db)
}

async fn remember(db: &Db, past: &Past<'_>) -> anyhow::Result<String> {
    let (prev_ago, since_prev) = past.prev.unzip();
    Ok(sqlx::query_scalar(
        "WITH st AS (
             SELECT s.queryid, sum(s.calls) AS calls, sum(s.rows) AS rows,
                    min(s.stats_since) AS since, min(s.query) AS query
               FROM pg_stat_statements s
              WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
                AND s.toplevel AND s.query = $1
              GROUP BY s.queryid
         ), run AS (
             INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome, findings)
             VALUES ('idle_polls', 'capture_db', 'regular', now(), 'fail', 1)
             RETURNING id
         ), p AS (
             INSERT INTO wright.problems (check_name, source, scope, resource, state, healthy_at)
             SELECT 'idle_polls', 'capture_db', 'query', 'query ' || st.queryid || ': ' || st.query,
                    CASE WHEN $2 THEN 'healthy' ELSE 'open' END, CASE WHEN $2 THEN now() END
               FROM st
             RETURNING id
         )
         INSERT INTO wright.episodes (problem_id, opened_at, last_seen_at, closed_at,
                                      opened_by_run, last_seen_run, closed_by_run, evidence)
         SELECT p.id, now() - $5::interval, now() - $3::interval,
                CASE WHEN $2 THEN now() - $3::interval + interval '5 minutes' END,
                run.id, run.id, CASE WHEN $2 THEN run.id END,
                jsonb_strip_nulls(jsonb_build_object(
                    'queryid', st.queryid::text, 'calls', st.calls - $4, 'rows', st.rows,
                    'total_seconds', 0, 'counted_since', st.since,
                    'calls_per_hour', $9::numeric,
                    'window_rows_per_call', CASE WHEN $9 IS NOT NULL THEN 0 END,
                    'base_at', now() - $5::interval, 'base_calls', st.calls - $6,
                    'base_rows', st.rows, 'base_total_ms', 0,
                    'prev_base_at', now() - $7::interval, 'prev_base_calls', st.calls - $8,
                    'prev_base_rows', CASE WHEN $8 IS NOT NULL THEN st.rows END,
                    'prev_base_total_ms', CASE WHEN $8 IS NOT NULL THEN 0 END))
           FROM st, p, run
         RETURNING evidence->>'queryid'",
    )
    .bind(POLL)
    .bind(past.closed)
    .bind(past.seen_ago)
    .bind(past.since_seen)
    .bind(past.base_ago)
    .bind(past.since_base)
    .bind(prev_ago)
    .bind(since_prev)
    .bind(past.carried_rate)
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

async fn record(db: &Db, findings: Vec<Finding>) -> anyhow::Result<()> {
    db.handle
        .web()
        .record_wright_run(&CheckRun {
            check: "idle_polls",
            source: "capture_db",
            cadence: "regular",
            started_at: Utc::now(),
            outcome: &CheckOutcome::Complete(findings),
            machine: None,
        })
        .await?;
    Ok(())
}

async fn age(db: &Db, by: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE wright.episodes e
            SET evidence = e.evidence || jsonb_build_object(
                    'base_at', (e.evidence->>'base_at')::timestamptz - $1::interval,
                    'prev_base_at', (e.evidence->>'prev_base_at')::timestamptz - $1::interval,
                    'observed_at', (e.evidence->>'observed_at')::timestamptz - $1::interval)
          WHERE e.closed_at IS NULL
            AND e.problem_id IN (SELECT id FROM wright.problems WHERE check_name = 'idle_polls')",
    )
    .bind(by)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

async fn problem_state(db: &Db) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT state FROM wright.problems WHERE check_name = 'idle_polls' ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(db.handle.pool())
    .await?)
}

fn past(closed: bool, base_ago: &str, since_base: i64) -> Past<'_> {
    Past {
        closed,
        seen_ago: "5 minutes",
        since_seen: 1,
        base_ago,
        since_base,
        prev: None,
        carried_rate: None,
    }
}

#[tokio::test]
async fn idle_polls_with_a_healed_problem_leaves_out_the_calls_before_it_healed()
-> anyhow::Result<()> {
    let db = boot(600).await?;
    let queryid = remember(
        &db,
        &Past {
            seen_ago: "50 minutes",
            since_seen: 2,
            ..past(true, "6 hours", 600)
        },
    )
    .await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_with_a_healed_problem_reports_polling_since_the_pass_that_last_flagged_it()
-> anyhow::Result<()> {
    let db = boot(600).await?;
    let queryid = remember(
        &db,
        &Past {
            seen_ago: "40 minutes",
            since_seen: 600,
            ..past(true, "6 hours", 600)
        },
    )
    .await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].evidence["judged_on"], "this window");
    assert_eq!(found[0].evidence["window_calls"], 600);
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_with_a_young_window_and_no_previous_one_reports_nothing() -> anyhow::Result<()>
{
    let db = boot(4).await?;
    let queryid = remember(
        &db,
        &Past {
            carried_rate: Some(301.1),
            ..past(false, "18 minutes", 4)
        },
    )
    .await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_with_a_young_window_and_a_quiet_previous_one_reports_nothing()
-> anyhow::Result<()> {
    let db = boot(4).await?;
    let queryid = remember(
        &db,
        &Past {
            prev: Some(("70 minutes", 4)),
            carried_rate: Some(147.0),
            ..past(false, "10 minutes", 1)
        },
    )
    .await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_with_a_young_window_judges_it_with_the_previous_one() -> anyhow::Result<()> {
    let db = boot(600).await?;
    let queryid = remember(
        &db,
        &Past {
            prev: Some(("70 minutes", 600)),
            ..past(false, "10 minutes", 100)
        },
    )
    .await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].evidence["judged_on"],
        "this and the previous window"
    );
    assert_eq!(found[0].evidence["window_calls"], 600);
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_on_a_statement_first_seen_seconds_ago_reports_nothing() -> anyhow::Result<()> {
    let db = boot(600).await?;
    assert_eq!(db.findings("idle_polls").await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn idle_polls_rolling_a_window_judges_the_closing_one_then_heals_once_calls_stop()
-> anyhow::Result<()> {
    let db = boot(600).await?;
    let queryid = remember(&db, &past(false, "65 minutes", 600)).await?;
    let rolled = reported(&db, &queryid).await?;
    assert_eq!(rolled.len(), 1);
    let evidence = &rolled[0].evidence;
    assert_eq!(evidence["judged_on"], "this window");
    assert_eq!(evidence["window_calls"], 600);
    assert_eq!(evidence["prev_base_at"], evidence["window_since"]);
    assert_eq!(evidence["base_calls"], evidence["calls"]);
    record(&db, rolled).await?;

    let next = reported(&db, &queryid).await?;
    assert_eq!(next.len(), 1);
    assert_eq!(
        next[0].evidence["judged_on"],
        "this and the previous window"
    );
    assert_eq!(next[0].evidence["window_calls"], 600);
    record(&db, next).await?;

    age(&db, "31 minutes").await?;
    let healed = reported(&db, &queryid).await?;
    assert_eq!(healed, Vec::new());
    record(&db, healed).await?;
    assert_eq!(problem_state(&db).await?, "healthy");
    db.cleanup().await
}
