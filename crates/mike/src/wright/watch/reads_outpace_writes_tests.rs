use super::fixtures_tests::Db;
use crate::wright::Finding;

const CHECK: &str = "reads_outpace_writes";
const LOOKUP: &str = "SELECT id, label FROM public.rare_lookup WHERE id >= $1";
const VIEW_LOOKUP: &str = "SELECT id, label FROM public.rare_view WHERE id >= $1";

async fn boot() -> anyhow::Result<Db> {
    let db = Db::boot().await?;
    let pool = db.handle.pool();
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(pool)
        .await?;
    sqlx::query(
        "UPDATE wright.checks
            SET params = params || jsonb_build_object('roles', jsonb_build_array(current_user))
          WHERE name IN ('idle_polls', 'reads_outpace_writes')",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE TABLE public.rare_lookup (id int PRIMARY KEY, label text NOT NULL)")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO public.rare_lookup VALUES (1, 'one'), (2, 'two')")
        .execute(pool)
        .await?;
    sqlx::query("CREATE VIEW public.rare_view AS SELECT id, label FROM public.rare_lookup")
        .execute(pool)
        .await?;
    Ok(db)
}

async fn read(db: &Db, sql: &'static str, times: usize) -> anyhow::Result<()> {
    for _ in 0..times {
        sqlx::query(sql).bind(0).fetch_all(db.handle.pool()).await?;
    }
    Ok(())
}

async fn snapshot(
    db: &Db,
    sql: &str,
    ago: &str,
    calls_since: i64,
    writes_since: Option<i64>,
) -> anyhow::Result<String> {
    let taken_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now() - $1::interval")
        .bind(ago)
        .fetch_one(db.handle.pool())
        .await?;
    let queryid: String = sqlx::query_scalar(
        "WITH st AS (
             SELECT s.queryid, sum(s.calls) AS calls, sum(s.rows) AS rows,
                    min(s.stats_since) AS since
               FROM pg_stat_statements s
              WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
                AND s.toplevel AND s.query = $1
              GROUP BY s.queryid
         )
         INSERT INTO wright.statement_counters (queryid, taken_at, calls, rows, total_ms, stats_since)
         SELECT st.queryid, $2, st.calls - $3, st.rows - 2 * $3, 0, st.since
           FROM st
         RETURNING queryid::text",
    )
    .bind(sql)
    .bind(taken_at)
    .bind(calls_since)
    .fetch_one(db.handle.pool())
    .await?;
    if let Some(writes) = writes_since {
        sqlx::query(
            "INSERT INTO wright.table_counters (taken_at, relid, writes)
             SELECT $1, t.relid,
                    t.n_tup_ins + t.n_tup_upd + t.n_tup_del
                        - CASE WHEN t.relname = 'rare_lookup' THEN $2 ELSE 0 END
               FROM pg_stat_user_tables t",
        )
        .bind(taken_at)
        .bind(writes)
        .execute(db.handle.pool())
        .await?;
    }
    Ok(queryid)
}

async fn reported(db: &Db, queryid: &str) -> anyhow::Result<Vec<Finding>> {
    Ok(db
        .findings(CHECK)
        .await?
        .into_iter()
        .filter(|f| f.evidence["queryid"] == queryid)
        .collect())
}

fn tables(found: &Finding) -> Vec<String> {
    found.evidence["tables"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["table"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn reads_outpace_writes_reports_a_lookup_of_a_table_nobody_writes() -> anyhow::Result<()> {
    let db = boot().await?;
    read(&db, LOOKUP, 600).await?;
    let queryid = snapshot(&db, LOOKUP, "40 minutes", 600, Some(0)).await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    let evidence = &found[0].evidence;
    assert_eq!(evidence["window_calls"], 600);
    assert_eq!(tables(&found[0]), ["public.rare_lookup"]);
    let rate = evidence["calls_per_hour"].as_f64().unwrap_or_default();
    assert!((890.0..=900.0).contains(&rate), "{rate}");
    let ratio = evidence["reads_per_write"].as_f64().unwrap_or_default();
    assert!(ratio >= 300.0, "{ratio}");
    assert_eq!(evidence["window_rows_per_call"], 2.0);
    db.cleanup().await
}

#[tokio::test]
async fn reads_outpace_writes_follows_a_view_to_its_table() -> anyhow::Result<()> {
    let db = boot().await?;
    read(&db, VIEW_LOOKUP, 600).await?;
    let queryid = snapshot(&db, VIEW_LOOKUP, "40 minutes", 600, Some(0)).await?;
    let found = reported(&db, &queryid).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(tables(&found[0]), ["public.rare_lookup"]);
    db.cleanup().await
}

#[tokio::test]
async fn reads_outpace_writes_leaves_a_lookup_whose_table_changes_as_often() -> anyhow::Result<()> {
    let db = boot().await?;
    read(&db, LOOKUP, 600).await?;
    let queryid = snapshot(&db, LOOKUP, "40 minutes", 600, Some(600)).await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn reads_outpace_writes_without_table_counters_for_the_window_judges_nothing()
-> anyhow::Result<()> {
    let db = boot().await?;
    read(&db, LOOKUP, 600).await?;
    let queryid = snapshot(&db, LOOKUP, "40 minutes", 600, None).await?;
    assert_eq!(reported(&db, &queryid).await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn reads_outpace_writes_saves_table_counters_with_each_statement_snapshot()
-> anyhow::Result<()> {
    let db = boot().await?;
    read(&db, LOOKUP, 10).await?;
    sqlx::query(
        "INSERT INTO wright.table_counters (taken_at, relid, writes)
         VALUES (now() - interval '3 hours', 1, 0)",
    )
    .execute(db.handle.pool())
    .await?;
    db.findings(CHECK).await?;
    let (snapshots, orphans): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE EXISTS (
                    SELECT 1 FROM wright.statement_counters c WHERE c.taken_at = t.taken_at)),
                count(*) FILTER (WHERE NOT EXISTS (
                    SELECT 1 FROM wright.statement_counters c WHERE c.taken_at = t.taken_at))
           FROM wright.table_counters t
          WHERE t.relid = 'public.rare_lookup'::regclass OR t.relid = 1",
    )
    .fetch_one(db.handle.pool())
    .await?;
    assert_eq!((snapshots, orphans), (1, 0));
    db.cleanup().await
}
