use chrono::Utc;
use serde_json::json;

use super::fixtures_tests::Db;
use super::reads_outpace_writes_tests as reads;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::wright::{CheckOutcome, CheckRun, Finding};

const REPLAY_FROM_OLD_VIEW: &str = "DROP VIEW IF EXISTS wright.cause_queue";
const MIGRATION: &str = include_str!(
    "../../../../mike-base/src/store/persistence/migrations/20261124150000_wright_query_checks_full_text.sql"
);
const RESOURCE_TEXT_CHARS: usize = 120;
const HUGE: usize = 25_000;

fn long_statement(tag: &str, padding: usize) -> String {
    format!(
        "SELECT p.id,   p.check_name,\n       p.scope\n  FROM wright.problems p\n WHERE p.id < $4 AND p.check_name <> $1 AND p.scope <> $2 AND p.resource <> $3\n   AND p.state <> p.scope /* {tag}: padding that keeps the statement far over 160 characters {} */",
        "w".repeat(padding)
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

fn named(id: &str, whole: &str) -> String {
    let cut: String = whole.chars().take(RESOURCE_TEXT_CHARS).collect();
    format!("query {id}: {cut}")
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

fn finding(scope: &str, resource: &str, evidence: serde_json::Value) -> Finding {
    Finding {
        scope: scope.to_owned(),
        resource: resource.to_owned(),
        evidence,
    }
}

async fn statement_of(db: &Db, resource: &str) -> anyhow::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT statement FROM wright.problems WHERE resource = $1")
            .bind(resource)
            .fetch_one(db.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn idle_polls_keeps_the_resource_short_and_puts_the_whole_statement_in_the_evidence()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    with_statements(&db).await?;
    let sql = long_statement("idle_polls full text", 300);
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
    assert_eq!(found[0].resource, named(&id, &whole));
    assert_eq!(found[0].evidence["query"], whole);
    db.cleanup().await
}

const LONG_LOOKUP: &str = "SELECT l.id,   l.label\n  FROM public.rare_lookup l /* written long so that the statement is far over the old 120 and 160 character cuts of the resource and the evidence */\n WHERE l.id >= $1\n ORDER BY l.id";

#[tokio::test]
async fn reads_outpace_writes_keeps_the_resource_short_and_puts_the_whole_statement_in_the_evidence()
-> anyhow::Result<()> {
    let db = reads::boot().await?;
    reads::read(&db, LONG_LOOKUP, 600).await?;
    let id = reads::snapshot(&db, LONG_LOOKUP, "40 minutes", 600, Some(0)).await?;
    let whole = collapsed(&db, LONG_LOOKUP).await?;
    assert!(whole.len() > 160, "{}", whole.len());
    let found = reads::reported(&db, &id).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resource, named(&id, &whole));
    assert_eq!(found[0].evidence["query"], whole);
    db.cleanup().await
}

#[tokio::test]
async fn slow_queries_reports_a_statement_over_20000_characters_whole_in_the_evidence()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    with_statements(&db).await?;
    sqlx::query(
        "UPDATE wright.checks SET params = params || '{\"mean_ms\": 0}'::jsonb
          WHERE name = 'slow_queries'",
    )
    .execute(db.handle.pool())
    .await?;
    let sql = long_statement("slow_queries full text", HUGE);
    run(&db, &sql, 25).await?;
    let id = queryid(&db, &sql).await?;
    let whole = collapsed(&db, &sql).await?;
    assert!(whole.len() > 20_000, "{}", whole.len());
    let found: Vec<Finding> = db
        .findings("slow_queries")
        .await?
        .into_iter()
        .filter(|f| f.evidence["queryid"] == id)
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resource, named(&id, &whole));
    assert_eq!(found[0].evidence["query"], sql);
    db.cleanup().await
}

#[tokio::test]
async fn no_stored_check_cuts_the_statement_in_the_evidence_or_the_comparison_text()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let cutting: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM wright.checks
          WHERE query LIKE '%''query'', left(%' OR query LIKE '%), 120) AS text_key%'
          ORDER BY name",
    )
    .fetch_all(db.handle.pool())
    .await?;
    assert_eq!(cutting, Vec::<String>::new());
    let resource_cut: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM wright.checks
          WHERE name IN ('slow_queries', 'idle_polls', 'reads_outpace_writes')
            AND query NOT LIKE '%|| left(regexp_replace(btrim(%'
          ORDER BY name",
    )
    .fetch_all(db.handle.pool())
    .await?;
    assert_eq!(resource_cut, Vec::<String>::new());
    db.cleanup().await
}

#[tokio::test]
async fn a_pass_stores_a_statement_over_20000_characters_on_the_problem_and_follows_a_change()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let whole = collapsed(&db, &long_statement("stored statement", HUGE)).await?;
    assert!(whole.len() > 20_000, "{}", whole.len());
    let resource = named("-621548", &whole);
    let evidence = |text: &str| json!({"queryid": "-621548", "query": text});
    let first = record(&db, vec![finding("query", &resource, evidence(&whole))]).await?;
    assert_eq!(first, (1, 0, 0, 0));
    assert_eq!(statement_of(&db, &resource).await?, Some(whole.clone()));
    let again = record(&db, vec![finding("query", &resource, evidence(&whole))]).await?;
    assert_eq!(again, (0, 0, 1, 0));
    let changed = format!("{whole} AND p.id > 0");
    record(&db, vec![finding("query", &resource, evidence(&changed))]).await?;
    assert_eq!(statement_of(&db, &resource).await?, Some(changed));
    let other = record(
        &db,
        vec![
            finding("query", &resource, json!({"queryid": "-621548"})),
            finding("capture", "not a statement", json!({"query": "SELECT 1"})),
        ],
    )
    .await?;
    assert_eq!(other, (1, 0, 1, 0));
    assert!(statement_of(&db, &resource).await?.is_some());
    assert_eq!(statement_of(&db, "not a statement").await?, None);
    db.cleanup().await
}

#[tokio::test]
async fn the_queue_and_the_problem_lists_carry_the_statement() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let whole = collapsed(&db, &long_statement("queue statement", HUGE)).await?;
    let resource = named("812345", &whole);
    record(
        &db,
        vec![finding(
            "query",
            &resource,
            json!({"queryid": "812345", "query": whole}),
        )],
    )
    .await?;
    db.handle.web().assign_wright_causes().await?;
    let causes = db.handle.web().list_wright_causes(10).await?;
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].statement.as_deref(), Some(whole.as_str()));
    assert_eq!(causes[0].resource, resource);
    let problems = db
        .handle
        .web()
        .wright_cause_problems(causes[0].cause_id)
        .await?
        .unwrap_or_default();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].statement.as_deref(), Some(whole.as_str()));
    let one = db.handle.web().wright_problem(problems[0].id).await?;
    assert_eq!(
        one.and_then(|p| p.statement).as_deref(),
        Some(whole.as_str())
    );
    db.cleanup().await
}

#[tokio::test]
async fn the_migration_fills_the_statement_of_a_query_problem_over_20000_characters()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    with_statements(&db).await?;
    let sql = long_statement("migration fill", HUGE);
    run(&db, &sql, 3).await?;
    let id = queryid(&db, &sql).await?;
    let whole = collapsed(&db, &sql).await?;
    assert!(whole.len() > 20_000, "{}", whole.len());
    let old = named(&id, &whole);
    let gone = "query 4242424242: SELECT something that pg_stat_statements no longer holds";
    let kept = "query 77: SELECT already stored";
    record(
        &db,
        vec![
            finding("query", &old, json!({"queryid": id})),
            finding("query", gone, json!({"queryid": "4242424242"})),
            finding(
                "query",
                kept,
                json!({"queryid": "77", "query": "SELECT already stored"}),
            ),
        ],
    )
    .await?;
    assert_eq!(statement_of(&db, &old).await?, None);

    sqlx::raw_sql(REPLAY_FROM_OLD_VIEW)
        .execute(db.handle.pool())
        .await?;
    sqlx::raw_sql(MIGRATION).execute(db.handle.pool()).await?;

    assert_eq!(statement_of(&db, &old).await?, Some(whole.clone()));
    assert_eq!(statement_of(&db, gone).await?, None);
    assert_eq!(
        statement_of(&db, kept).await?.as_deref(),
        Some("SELECT already stored")
    );
    let again = record(
        &db,
        vec![finding(
            "query",
            &old,
            json!({"queryid": id, "query": whole}),
        )],
    )
    .await?;
    assert_eq!(again, (0, 0, 1, 2));
    sqlx::raw_sql(REPLAY_FROM_OLD_VIEW)
        .execute(db.handle.pool())
        .await?;
    sqlx::raw_sql(MIGRATION).execute(db.handle.pool()).await?;
    assert_eq!(statement_of(&db, &old).await?, Some(whole));
    db.cleanup().await
}
