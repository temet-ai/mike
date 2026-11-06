use super::*;
use crate::store::persistence::test_support;

async fn set_closed_hours_ago(pool: &sqlx::PgPool, turn_id: i64, hours: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.operator_turns
            SET closed_at = NOW() - ($2::int || ' hours')::interval
          WHERE id = $1",
    )
    .bind(turn_id)
    .bind(hours)
    .execute(pool)
    .await?;
    Ok(())
}

async fn turn_ids_of(pool: &sqlx::PgPool, project_id: i64) -> anyhow::Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM memory.operator_turns WHERE project_id = $1 ORDER BY id ASC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?)
}

#[tokio::test]
async fn unjudged_backlog_counts_only_old_eligible_turns() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "w-unjudged-backlog").await?;
    let session = "00000000-0000-4000-8000-0000000000f1";
    test_support::seed_two_sessions(
        &pool,
        session,
        "00000000-0000-4000-8000-0000000000f2",
        project_id,
    )
    .await?;
    test_support::seed_session_turns(&pool, session, project_id, 4).await?;
    let ids = turn_ids_of(&pool, project_id).await?;
    let &[old_unjudged, fresh_unjudged, old_judged, old_permanent] = &ids[..] else {
        anyhow::bail!("expected four seeded turns, got {ids:?}");
    };

    set_closed_hours_ago(&pool, old_unjudged, 10).await?;
    set_closed_hours_ago(&pool, old_judged, 10).await?;
    set_closed_hours_ago(&pool, old_permanent, 10).await?;
    sqlx::query("UPDATE memory.operator_turns SET judge_model = 'test-model' WHERE id = $1")
        .bind(old_judged)
        .execute(&pool)
        .await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET judge_failure_reason = 'permanent' WHERE id = $1",
    )
    .bind(old_permanent)
    .execute(&pool)
    .await?;

    let cutoff = chrono::Utc::now() - chrono::Duration::hours(6);
    let backlog = handle.unjudged_closed_turn_backlog(cutoff).await?;

    assert_eq!(
        backlog.stale_turns, 1,
        "only the old, unjudged, non-permanent turn counts (fresh {fresh_unjudged}, judged {old_judged}, permanent {old_permanent} do not)"
    );
    let oldest = backlog
        .oldest_closed_at
        .ok_or_else(|| anyhow::anyhow!("a non-empty backlog must report its oldest closed_at"))?;
    assert!(
        oldest < cutoff,
        "the oldest unjudged turn must predate the cutoff, got {oldest}"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unjudged_backlog_is_empty_when_every_turn_is_fresh() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "w-unjudged-fresh").await?;
    let session = "00000000-0000-4000-8000-0000000000f3";
    test_support::seed_two_sessions(
        &pool,
        session,
        "00000000-0000-4000-8000-0000000000f4",
        project_id,
    )
    .await?;
    test_support::seed_session_turns(&pool, session, project_id, 2).await?;

    let cutoff = chrono::Utc::now() - chrono::Duration::hours(6);
    let backlog = handle.unjudged_closed_turn_backlog(cutoff).await?;

    assert_eq!(backlog.stale_turns, 0);
    assert_eq!(backlog.oldest_closed_at, None);

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
