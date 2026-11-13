use crate::test_db;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

const MIGRATION_SQL: &str = include_str!(
    "../../../mike-base/src/store/persistence/migrations/20260928110000_llm_daily_spend_rollup.sql"
);

async fn connect(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(2).connect(url).await?)
}

async fn revert_migration(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::raw_sql(
        "DROP TRIGGER IF EXISTS trg_llm_calls_daily_spend ON llm.calls;
         DROP FUNCTION IF EXISTS llm.apply_call_to_daily_spend();
         DROP TABLE IF EXISTS llm.daily_spend;",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn table_and_trigger_exist(pool: &PgPool) -> anyhow::Result<(bool, bool)> {
    let table: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                          WHERE table_schema = 'llm' AND table_name = 'daily_spend')",
    )
    .fetch_one(pool)
    .await?;
    let trigger: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'trg_llm_calls_daily_spend')",
    )
    .fetch_one(pool)
    .await?;
    Ok((table, trigger))
}

struct SeedCall {
    created_at: &'static str,
    role: &'static str,
    steerer_id: Option<i64>,
    model: &'static str,
    cost_usd: f64,
    prompt_tokens: i32,
    completion_tokens: i32,
    reasoning_tokens: i32,
    outcome: &'static str,
}

async fn insert_call(pool: &PgPool, call: &SeedCall) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO llm.calls
             (created_at, role, model, cost_usd, prompt_tokens, completion_tokens,
              reasoning_tokens, outcome, steerer_id)
         VALUES ($1::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(call.created_at)
    .bind(call.role)
    .bind(call.model)
    .bind(call.cost_usd)
    .bind(call.prompt_tokens)
    .bind(call.completion_tokens)
    .bind(call.reasoning_tokens)
    .bind(call.outcome)
    .bind(call.steerer_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
struct DailySpendRow {
    day: chrono::NaiveDate,
    role: String,
    steerer_id: Option<i64>,
    model: String,
    call_count: i64,
    cost_usd: f64,
    prompt_tokens: i64,
    completion_tokens: i64,
    reasoning_tokens: i64,
    failed_count: i64,
}

async fn daily_spend_rows(pool: &PgPool) -> anyhow::Result<Vec<DailySpendRow>> {
    Ok(sqlx::query_as::<_, DailySpendRow>(
        "SELECT day, role, steerer_id, model, call_count, cost_usd,
                prompt_tokens, completion_tokens, reasoning_tokens, failed_count
           FROM llm.daily_spend
          ORDER BY day, role, steerer_id, model",
    )
    .fetch_all(pool)
    .await?)
}

async fn raw_aggregate_rows(pool: &PgPool) -> anyhow::Result<Vec<DailySpendRow>> {
    Ok(sqlx::query_as::<_, DailySpendRow>(
        "SELECT (created_at AT TIME ZONE 'utc')::date AS day, role, steerer_id, model,
                COUNT(*)::bigint AS call_count,
                COALESCE(SUM(cost_usd), 0.0)::float8 AS cost_usd,
                COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
                COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
                COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens,
                COUNT(*) FILTER (WHERE outcome NOT IN ('ok', 'truncated'))::bigint AS failed_count
           FROM llm.calls
          GROUP BY 1, 2, 3, 4
          ORDER BY 1, 2, 3, 4",
    )
    .fetch_all(pool)
    .await?)
}

#[tokio::test]
async fn llm_daily_spend_rollup_applies_cleanly_to_fresh_empty_db() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping llm_daily_spend_rollup fresh-db test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = connect(&handle.database_url).await?;

    revert_migration(&pool).await?;
    assert_eq!(table_and_trigger_exist(&pool).await?, (false, false));

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must apply cleanly against an empty llm.calls table")?;
    assert_eq!(table_and_trigger_exist(&pool).await?, (true, true));
    assert!(daily_spend_rows(&pool).await?.is_empty());

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must be safely re-runnable")?;
    assert!(daily_spend_rows(&pool).await?.is_empty());

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

fn two_day_seed_calls() -> [SeedCall; 6] {
    [
        SeedCall {
            created_at: "2026-09-20T10:00:00Z",
            role: "steerer",
            steerer_id: Some(9001),
            model: "model-a",
            cost_usd: 0.02,
            prompt_tokens: 100,
            completion_tokens: 10,
            reasoning_tokens: 0,
            outcome: "ok",
        },
        SeedCall {
            created_at: "2026-09-20T11:00:00Z",
            role: "steerer",
            steerer_id: Some(9001),
            model: "model-a",
            cost_usd: 0.03,
            prompt_tokens: 200,
            completion_tokens: 20,
            reasoning_tokens: 5,
            outcome: "http_error",
        },
        SeedCall {
            created_at: "2026-09-20T12:00:00Z",
            role: "steerer",
            steerer_id: Some(9002),
            model: "model-a",
            cost_usd: 0.01,
            prompt_tokens: 50,
            completion_tokens: 5,
            reasoning_tokens: 0,
            outcome: "ok",
        },
        SeedCall {
            created_at: "2026-09-20T13:00:00Z",
            role: "judge_worker",
            steerer_id: None,
            model: "model-b",
            cost_usd: 0.05,
            prompt_tokens: 300,
            completion_tokens: 30,
            reasoning_tokens: 0,
            outcome: "ok",
        },
        SeedCall {
            created_at: "2026-09-21T09:00:00Z",
            role: "steerer",
            steerer_id: Some(9001),
            model: "model-a",
            cost_usd: 0.04,
            prompt_tokens: 150,
            completion_tokens: 15,
            reasoning_tokens: 0,
            outcome: "truncated",
        },
        SeedCall {
            created_at: "2026-09-21T09:30:00Z",
            role: "judge_worker",
            steerer_id: None,
            model: "model-b",
            cost_usd: 0.02,
            prompt_tokens: 100,
            completion_tokens: 10,
            reasoning_tokens: 0,
            outcome: "timeout",
        },
    ]
}

fn assert_backfilled_buckets(rolled_up: &[DailySpendRow]) -> anyhow::Result<()> {
    assert_eq!(
        rolled_up.len(),
        5,
        "five distinct (day, role, steerer, model) buckets"
    );

    let day1_9001 = rolled_up
        .iter()
        .find(|r| r.steerer_id == Some(9001) && r.day.to_string() == "2026-09-20")
        .ok_or_else(|| anyhow::anyhow!("day1/steerer 9001 bucket must exist"))?;
    assert_eq!(day1_9001.call_count, 2);
    assert!((day1_9001.cost_usd - 0.05).abs() < 1e-9);
    assert_eq!(day1_9001.prompt_tokens, 300);
    assert_eq!(day1_9001.reasoning_tokens, 5);
    assert_eq!(
        day1_9001.failed_count, 1,
        "the http_error call must count as failed"
    );

    let day2_judge = rolled_up
        .iter()
        .find(|r| r.role == "judge_worker" && r.day.to_string() == "2026-09-21")
        .ok_or_else(|| anyhow::anyhow!("day2/judge_worker bucket must exist"))?;
    assert_eq!(day2_judge.failed_count, 1, "a timeout must count as failed");

    let day2_steerer = rolled_up
        .iter()
        .find(|r| r.steerer_id == Some(9001) && r.day.to_string() == "2026-09-21")
        .ok_or_else(|| anyhow::anyhow!("day2/steerer 9001 bucket must exist"))?;
    assert_eq!(
        day2_steerer.failed_count, 0,
        "a truncated outcome must not count as failed"
    );
    Ok(())
}

async fn insert_post_backfill_calls(pool: &PgPool) -> anyhow::Result<()> {
    let new_bucket = SeedCall {
        created_at: "2026-09-22T08:00:00Z",
        role: "session_judge",
        steerer_id: None,
        model: "model-c",
        cost_usd: 0.005,
        prompt_tokens: 40,
        completion_tokens: 4,
        reasoning_tokens: 0,
        outcome: "ok",
    };
    insert_call(pool, &new_bucket).await?;

    let existing_bucket_update = SeedCall {
        created_at: "2026-09-20T14:00:00Z",
        role: "steerer",
        steerer_id: Some(9001),
        model: "model-a",
        cost_usd: 0.10,
        prompt_tokens: 1_000,
        completion_tokens: 100,
        reasoning_tokens: 50,
        outcome: "ok",
    };
    insert_call(pool, &existing_bucket_update).await?;
    Ok(())
}

fn assert_trigger_folded_buckets(after_trigger: &[DailySpendRow]) -> anyhow::Result<()> {
    assert_eq!(
        after_trigger.len(),
        6,
        "the new bucket must appear alongside the five backfilled ones"
    );

    let updated_day1_9001 = after_trigger
        .iter()
        .find(|r| r.steerer_id == Some(9001) && r.day.to_string() == "2026-09-20")
        .ok_or_else(|| anyhow::anyhow!("day1/steerer 9001 bucket must still exist"))?;
    assert_eq!(updated_day1_9001.call_count, 3);
    assert!((updated_day1_9001.cost_usd - 0.15).abs() < 1e-9);
    assert_eq!(updated_day1_9001.prompt_tokens, 1_300);
    assert_eq!(updated_day1_9001.reasoning_tokens, 55);
    assert_eq!(
        updated_day1_9001.failed_count, 1,
        "the new 'ok' call must not increment failed_count"
    );
    Ok(())
}

#[tokio::test]
async fn llm_daily_spend_rollup_backfills_seeded_prior_state_exactly() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping llm_daily_spend_rollup seeded test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = connect(&handle.database_url).await?;

    revert_migration(&pool).await?;
    for call in &two_day_seed_calls() {
        insert_call(&pool, call).await?;
    }

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must backfill cleanly over existing llm.calls rows")?;

    let rolled_up = daily_spend_rows(&pool).await?;
    let raw = raw_aggregate_rows(&pool).await?;
    assert_eq!(
        rolled_up, raw,
        "backfilled llm.daily_spend must equal a raw GROUP BY over llm.calls"
    );
    assert_backfilled_buckets(&rolled_up)?;

    insert_post_backfill_calls(&pool).await?;

    let after_trigger = daily_spend_rows(&pool).await?;
    let raw_after = raw_aggregate_rows(&pool).await?;
    assert_eq!(
        after_trigger, raw_after,
        "post-migration inserts must be folded into the rollup exactly, via the trigger"
    );
    assert_trigger_folded_buckets(&after_trigger)?;

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("re-running the migration over rolled-up data must be harmless")?;
    let after_rerun = daily_spend_rows(&pool).await?;
    assert_eq!(
        after_rerun, after_trigger,
        "re-running the migration must not double-count via the ON CONFLICT DO NOTHING backfill"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
