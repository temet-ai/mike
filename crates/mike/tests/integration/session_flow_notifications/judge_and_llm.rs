use super::fixtures::{
    assert_no_more, open_listener, recv_matching, recv_payload, seed_project_and_session,
};

use crate::test_db;

use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn session_judges_verdicts_routes_session_then_global_on_cascade() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (_project_id, session_pk, session_id) = seed_project_and_session(&pool).await?;
    let mut listener = open_listener(&handle.database_url).await?;

    let verdict_id: i64 = sqlx::query_scalar(
        "INSERT INTO session_judges.verdicts
            (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version)
         SELECT $1, oc.id, 0.9::numeric(4,3), 'good', 'test-judge', 1
           FROM session_judges.outcomes oc WHERE oc.code = 'success'
         RETURNING id",
    )
    .bind(session_pk)
    .fetch_one(&pool)
    .await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "session_judges_verdicts");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], verdict_id.to_string());
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    sqlx::query("DELETE FROM sessions.sessions WHERE id = $1")
        .bind(session_pk)
        .execute(&pool)
        .await?;
    let payload = recv_matching(&mut listener, "session_judges_verdicts").await?;
    assert_eq!(payload["op"], "DELETE");
    assert_eq!(payload["id"], verdict_id.to_string());
    assert!(
        payload.get("session_id").is_none_or(Value::is_null),
        "a same-statement cascade delete of the parent session must route global; payload was {payload}"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn llm_calls_routes_nullable_non_fk_session_id() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (_project_id, session_pk, session_id) = seed_project_and_session(&pool).await?;
    let mut listener = open_listener(&handle.database_url).await?;

    let with_session_id: i64 = sqlx::query_scalar(
        "INSERT INTO llm.calls (role, model, outcome, session_id)
         VALUES ('steerer', 'test-model', 'ok', $1)
         RETURNING id",
    )
    .bind(session_pk)
    .fetch_one(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "llm_calls");
    assert_eq!(payload["id"], with_session_id.to_string());
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    let without_session_id: i64 = sqlx::query_scalar(
        "INSERT INTO llm.calls (role, model, outcome, session_id)
         VALUES ('lesson_relations', 'test-model', 'ok', NULL)
         RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["id"], without_session_id.to_string());
    assert!(payload.get("session_id").is_none_or(Value::is_null));
    assert_no_more(&mut listener).await?;

    let unresolvable_session_id: i64 = sqlx::query_scalar(
        "INSERT INTO llm.calls (role, model, outcome, session_id)
         VALUES ('judge_worker', 'test-model', 'ok', 999999999)
         RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["id"], unresolvable_session_id.to_string());
    assert!(
        payload.get("session_id").is_none_or(Value::is_null),
        "an id with no matching session row (no FK enforces one) must route global; payload was {payload}"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
