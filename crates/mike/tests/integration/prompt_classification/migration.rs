use anyhow::Context;
use chrono::Utc;
use mike::store::test_support::{self, TestDb, TestDbHandle};
use sqlx::PgPool;

use super::{NewRequest, insert_request, insert_session, stored_classification};

const MIGRATION_SQL: &str = include_str!(
    "../../../src/store/persistence/migrations/20260924100000_prompt_classification_unified.sql"
);

async fn migration_pool(test_db: &TestDb) -> anyhow::Result<(TestDbHandle, PgPool)> {
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPool::connect(&handle.database_url).await?;
    Ok((handle, pool))
}

const KIND_CODES: [&str; 8] = [
    "loop-wakeup",
    "channel-wake",
    "task-notification",
    "tool-loaded",
    "mailbox-delivery",
    "steering-payload",
    "harness-authored",
    "agent-lineage",
];

async fn new_kind_ids(pool: &PgPool) -> anyhow::Result<Vec<(i64, String)>> {
    let rows = sqlx::query_as(
        "SELECT id, code FROM prompt_kinds.kinds WHERE id BETWEEN 10 AND 17 ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[tokio::test]
async fn migration_prompt_classification_unified_adds_kinds_10_to_17_and_columns()
-> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping prompt_classification migration test");
        return Ok(());
    };
    let (handle, pool) = migration_pool(&test_db).await?;
    sqlx::raw_sql(
        "ALTER TABLE prompt_kinds.classifications
             DROP COLUMN rule, DROP COLUMN repeat_key, DROP COLUMN classifier_version;
         ALTER TABLE memory.operator_turns DROP COLUMN opened_by;
         DELETE FROM prompt_kinds.kinds WHERE id BETWEEN 10 AND 17;",
    )
    .execute(&pool)
    .await?;
    let project_id = test_support::seed_project(&pool, "migration-prior-state").await?;
    let session_id = insert_session(&pool, project_id, Utc::now()).await?;
    let request_id: i64 = 9001;
    insert_request(
        &pool,
        &NewRequest {
            id: request_id,
            session_id,
            project_id,
            at: Utc::now(),
            human_prompt: "prior state prompt",
            author: "operator",
            agent_id: None,
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT $1, $2, k.id FROM prompt_kinds.kinds k WHERE k.code = 'operator'",
    )
    .bind(request_id)
    .bind(session_id)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id, user_prompt)
         VALUES ($1, $2, $3, $3, 'prior state prompt')",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(request_id)
    .execute(&pool)
    .await?;
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must apply over prior state with rows")?;
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must be safely re-runnable")?;
    let kinds = new_kind_ids(&pool).await?;
    let codes: Vec<&str> = kinds.iter().map(|(_, code)| code.as_str()).collect();
    assert_eq!(codes, KIND_CODES);
    assert_eq!(kinds.first().map(|(id, _)| *id), Some(10));
    let kept = stored_classification(&pool, request_id)
        .await?
        .context("prior classification row must survive")?;
    assert_eq!(kept, ("operator".to_string(), None, None, None));
    let opened_by: Option<String> = sqlx::query_scalar(
        "SELECT opened_by FROM memory.operator_turns WHERE start_request_id = $1",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(opened_by, None);
    let next_id: i64 = sqlx::query_scalar(
        "INSERT INTO prompt_kinds.kinds (code) VALUES ('probe-next-id') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    assert!(next_id > 17, "identity must sit above the seeded ids");
    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
