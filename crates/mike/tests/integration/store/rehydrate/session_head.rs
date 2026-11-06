use chrono::Utc;

use mike::store::persistence::rehydrate::fetch_session_head;

use crate::store::rehydrate::support::{
    seed_project, seed_request_with_status, seed_session, shared_pool, unique,
};

#[tokio::test]
async fn fetch_session_head_derives_last_status_live_not_from_stale_cache() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let project_id = seed_project(&pool, &unique("session-head-proj")).await?;

    let in_flight_session = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET last_status = 'Pending' WHERE id = $1",
        in_flight_session,
    )
    .execute(&pool)
    .await?;
    seed_request_with_status(&pool, in_flight_session, project_id, "pending", None, None).await?;

    let complete_session = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET last_status = 'Pending' WHERE id = $1",
        complete_session,
    )
    .execute(&pool)
    .await?;
    seed_request_with_status(
        &pool,
        complete_session,
        project_id,
        "complete",
        Some(Utc::now()),
        None,
    )
    .await?;

    let error_session = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET last_status = 'Pending' WHERE id = $1",
        error_session,
    )
    .execute(&pool)
    .await?;
    seed_request_with_status(
        &pool,
        error_session,
        project_id,
        "error",
        Some(Utc::now()),
        Some("aborted: cleanup-on-drop before terminal event"),
    )
    .await?;

    let in_flight_head = fetch_session_head(&pool, in_flight_session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("in-flight session must have a head row"))?;
    assert_eq!(
        in_flight_head.last_status.as_deref(),
        Some("Pending"),
        "genuinely non-terminal request must still read Pending"
    );

    let complete_head = fetch_session_head(&pool, complete_session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("completed session must have a head row"))?;
    assert_eq!(
        complete_head.last_status.as_deref(),
        Some("Complete"),
        "completed request must not read the stale Pending cache"
    );

    let error_head = fetch_session_head(&pool, error_session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("errored session must have a head row"))?;
    assert_eq!(
        error_head.last_status.as_deref(),
        Some("Error: aborted: cleanup-on-drop before terminal event"),
        "terminated-with-error request must surface as Error, never Pending — issue #142"
    );

    pool.close().await;
    Ok(())
}
