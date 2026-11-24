use chrono::Utc;
use serde_json::json;

use super::fixtures_tests::Db;
use super::reads_outpace_writes_tests as reads;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::wright::{CheckOutcome, CheckRun, Finding};

const MIGRATION: &str = include_str!(
    "../../../../mike-base/src/store/persistence/migrations/20261124150000_wright_query_checks_full_text.sql"
);

fn long_statement(tag: &str) -> String {
    format!(
        "SELECT p.id,   p.check_name,\n       p.scope\n  FROM wright.problems p\n WHERE p.id < $4 AND p.check_name <> $1 AND p.scope <> $2 AND p.resource <> $3\n   AND p.state <> p.scope /* {tag}: padding that keeps the statement far over 160 characters {} */",
        "w".repeat(300)
    )
}

async fn with_statements(db: &Db) -> anyhow::Result<()> {
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
    Ok(())
}

async fn run(db: &Db, sql: &str, times: usize) -> anyhow::Result<()> {
    for _ in 0..times {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind("none")
            .bind("none")
            .bind("none")
            .bind(0_i64)
            .fetch_all(db.handle.pool())
            .await?;
    }
    Ok(())
}

async fn collapsed(db: &Db, sql: &str) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT regexp_replace(btrim($1), '\\s+', ' ', 'g')")
            .bind(sql)
            .fetch_one(db.handle.pool())
            .await?,
    )
}

async fn queryid(db: &Db, sql: &str) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT s.queryid::text
           FROM pg_stat_statements s
          WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
            AND s.toplevel AND s.query = $1
          LIMIT 1",
    )
    .bind(sql)
    .fetch_one(db.handle.pool())
    .await?)
}

async fn idle_snapshot(db: &Db, sql: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO wright.statement_counters (queryid, taken_at, calls, rows, total_ms, stats_since)
         SELECT s.queryid, now() - interval '40 minutes', s.calls - 600, s.rows, 0, s.stats_since
           FROM pg_stat_statements s
          WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
            AND s.toplevel AND s.query = $1",
    )
    .bind(sql)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

async fn record(db: &Db, findings: Vec<Finding>) -> anyhow::Result<(i64, i64, i64, i64)> {
    let outcome = CheckOutcome::Complete(findings);
    let recorded = db
        .handle
        .web()
        .record_wright_run(&CheckRun {
            check: "idle_polls",
            source: "capture_db",
            cadence: "regular",
            started_at: Utc::now(),
            outcome: &outcome,
            machine: None,
        })
        .await?;
    Ok((
        recorded.opened,
        recorded.recurred,
        recorded.still_open,
        recorded.healthy,
    ))
}

fn finding(resource: &str) -> Finding {
    Finding {
        scope: "query".to_owned(),
        resource: resource.to_owned(),
        evidence: json!({"queryid": "x"}),
    }
}

async fn resource_of(db: &Db, problem: i64) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT resource FROM wright.problems WHERE id = $1")
            .bind(problem)
            .fetch_one(db.handle.pool())
            .await?,
    )
}

async fn problem_id(db: &Db, resource: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM wright.problems WHERE resource = $1")
            .bind(resource)
            .fetch_one(db.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn idle_polls_reports_the_whole_statement_in_resource_and_evidence() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    with_statements(&db).await?;
    let sql = long_statement("idle_polls full text");
    run(&db, &sql, 600).await?;
    idle_snapshot(&db, &sql).await?;
    let id = queryid(&db, &sql).await?;
    let whole = collapsed(&db, &sql).await?;
    assert!(whole.len() > 400, "{}", whole.len());
    let found: Vec<Finding> = db
        .findings("idle_polls")
        .await?
        .into_iter()
        .filter(|f| f.evidence["queryid"] == id)
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resource, format!("query {id}: {whole}"));
    assert_eq!(found[0].evidence["query"], whole);
    db.cleanup().await
}

const LONG_LOOKUP: &str = "SELECT l.id,   l.label\n  FROM public.rare_lookup l /* written long so that the statement is far over the old 120 and 160 character cuts of the resource and the evidence */\n WHERE l.id >= $1\n ORDER BY l.id";

#[tokio::test]
async fn reads_outpace_writes_reports_the_whole_statement_in_resource_and_evidence()
-> anyhow::Result<()> {
    let db = reads::boot().await?;
    reads::read(&db, LONG_LOOKUP, 600).await?;
    let id = reads::snapshot(&db, LONG_LOOKUP, "40 minutes", 600, Some(0)).await?;
    let whole = collapsed(&db, LONG_LOOKUP).await?;
    assert!(whole.len() > 160, "{}", whole.len());
    let found = reads::reported(&db, &id).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resource, format!("query {id}: {whole}"));
    assert_eq!(found[0].evidence["query"], whole);
    db.cleanup().await
}

#[tokio::test]
async fn slow_queries_reports_a_statement_over_4000_characters_whole() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    with_statements(&db).await?;
    sqlx::query(
        "UPDATE wright.checks SET params = params || '{\"mean_ms\": 0}'::jsonb
          WHERE name = 'slow_queries'",
    )
    .execute(db.handle.pool())
    .await?;
    let mut sql = long_statement("slow_queries full text");
    sql.push_str(&format!(" /* {} */", "z".repeat(4400)));
    run(&db, &sql, 25).await?;
    let id = queryid(&db, &sql).await?;
    let whole = collapsed(&db, &sql).await?;
    assert!(whole.len() > 4400, "{}", whole.len());
    let found: Vec<Finding> = db
        .findings("slow_queries")
        .await?
        .into_iter()
        .filter(|f| f.evidence["queryid"] == id)
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resource, format!("query {id}: {whole}"));
    assert_eq!(found[0].evidence["query"], sql);
    db.cleanup().await
}

#[tokio::test]
async fn no_stored_check_cuts_the_statement_text() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let cutting: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM wright.checks
          WHERE query ~ 'left\\(regexp_replace\\(btrim\\(\\w+\\.query\\)|left\\(\\w+\\.query, '
          ORDER BY name",
    )
    .fetch_all(db.handle.pool())
    .await?;
    assert_eq!(cutting, Vec::<String>::new());
    db.cleanup().await
}

#[tokio::test]
async fn the_migration_rewrites_a_truncated_query_problem_so_the_next_pass_keeps_it()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    with_statements(&db).await?;
    let sql = long_statement("migration rewrite");
    run(&db, &sql, 3).await?;
    let id = queryid(&db, &sql).await?;
    let whole = collapsed(&db, &sql).await?;
    let cut: String = whole.chars().take(120).collect();
    let old = format!("query {id}: {cut}");
    let gone = "query 4242424242: SELECT something that pg_stat_statements no longer holds".to_owned();
    record(&db, vec![finding(&old), finding(&gone)]).await?;
    let (kept, left_alone) = (problem_id(&db, &old).await?, problem_id(&db, &gone).await?);

    sqlx::raw_sql(MIGRATION).execute(db.handle.pool()).await?;

    let full = format!("query {id}: {whole}");
    assert_eq!(resource_of(&db, kept).await?, full);
    assert_eq!(resource_of(&db, left_alone).await?, gone);
    let (opened, recurred, still_open, healed) = record(&db, vec![finding(&full)]).await?;
    assert_eq!((opened, recurred, still_open, healed), (0, 0, 1, 1));
    assert_eq!(problem_id(&db, &full).await?, kept);
    let open_episodes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wright.episodes WHERE problem_id = $1 AND closed_at IS NULL",
    )
    .bind(kept)
    .fetch_one(db.handle.pool())
    .await?;
    assert_eq!(open_episodes, 1);
    sqlx::raw_sql(MIGRATION).execute(db.handle.pool()).await?;
    assert_eq!(resource_of(&db, kept).await?, full);
    db.cleanup().await
}

#[tokio::test]
async fn triage_gives_the_full_text_the_cause_the_truncated_text_already_has() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let whole = collapsed(&db, &long_statement("triage key")).await?;
    let cut: String = whole.chars().take(120).collect();
    let truncated = format!("query -621548: {cut}");
    let full = format!("query 812345: {whole}");
    record(&db, vec![finding(&truncated)]).await?;
    db.handle.web().assign_wright_causes().await?;
    record(&db, vec![finding(&truncated), finding(&full)]).await?;
    db.handle.web().assign_wright_causes().await?;
    let causes: Vec<Option<i64>> = sqlx::query_scalar(
        "SELECT cause_id FROM wright.problems WHERE resource IN ($1, $2) ORDER BY resource",
    )
    .bind(&truncated)
    .bind(&full)
    .fetch_all(db.handle.pool())
    .await?;
    assert_eq!(causes.len(), 2);
    assert!(causes[0].is_some());
    assert_eq!(causes[0], causes[1]);
    let keys: i64 = sqlx::query_scalar("SELECT count(DISTINCT key) FROM wright.causes")
        .fetch_one(db.handle.pool())
        .await?;
    assert_eq!(keys, 1);
    db.cleanup().await
}
