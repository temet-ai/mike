use super::fixture;

const MIGRATION: &str = include_str!(
    "../../../src/store/persistence/migrations/20261005000000_learning_proposal_queue.sql"
);

#[tokio::test]
async fn learning_queue_migration_replays_on_fresh_schema() -> anyhow::Result<()> {
    let Some(f) = fixture("learning-migration-fresh").await? else {
        return Ok(());
    };
    sqlx::raw_sql(MIGRATION).execute(f.handle.pool()).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.lesson_proposal_jobs")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(count, 0);
    let invalid = sqlx::query(
        "INSERT INTO memory.lesson_proposer_daily_budget (day, consumed) VALUES ('2001-01-01', -1)",
    )
    .execute(f.handle.pool())
    .await;
    assert!(
        invalid.is_err(),
        "negative usage must be rejected by the schema"
    );
    Ok(())
}

#[tokio::test]
async fn learning_queue_migration_preserves_existing_budget_and_seeds_history() -> anyhow::Result<()>
{
    let Some(f) = fixture("learning-migration-upgrade").await? else {
        return Ok(());
    };
    sqlx::query("INSERT INTO llm.calls (role,model,outcome,created_at) VALUES ('lesson_proposer','test','ok','2001-01-01T12:00:00Z'), ('lesson_proposer','test','ok','2001-01-02T12:00:00Z')")
        .execute(f.handle.pool()).await?;
    sqlx::query(
        "INSERT INTO memory.lesson_proposer_daily_budget (day,consumed) VALUES ('2001-01-01',7)",
    )
    .execute(f.handle.pool())
    .await?;
    sqlx::raw_sql(MIGRATION).execute(f.handle.pool()).await?;
    sqlx::raw_sql(MIGRATION).execute(f.handle.pool()).await?;
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT day::text,consumed FROM memory.lesson_proposer_daily_budget WHERE day BETWEEN '2001-01-01' AND '2001-01-02' ORDER BY day")
        .fetch_all(f.handle.pool()).await?;
    assert_eq!(
        rows,
        vec![("2001-01-01".into(), 7), ("2001-01-02".into(), 1)]
    );
    Ok(())
}
