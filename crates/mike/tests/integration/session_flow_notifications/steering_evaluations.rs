use super::fixtures::{
    assert_no_more, open_listener, recv_matching, recv_payload, seed_project_and_session,
};

use crate::test_db;

use mike::store::test_support::seed_session_turns;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn steering_evaluations_routes_session_then_global_on_cascade() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (project_id, session_pk, session_id) = seed_project_and_session(&pool).await?;
    seed_session_turns(&pool, &session_id, project_id, 1).await?;
    let operator_turn_id: i64 =
        sqlx::query_scalar("SELECT id FROM memory.operator_turns WHERE session_id = $1")
            .bind(session_pk)
            .fetch_one(&pool)
            .await?;

    let mut listener = open_listener(&handle.database_url).await?;

    let eval_uuid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO steering.evaluations (steerer_id, session_id, operator_turn_id, status)
         SELECT st.id, $1, $2, 'dispatched'
           FROM steering.steerers st WHERE st.kind = 'verifier'
         RETURNING uuid",
    )
    .bind(session_pk)
    .bind(operator_turn_id)
    .fetch_one(&pool)
    .await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "steering_evaluations");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], eval_uuid.to_string());
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    sqlx::query(
        "UPDATE steering.evaluations SET status = 'resolved_pass', resolved_at = now() WHERE uuid = $1",
    )
    .bind(eval_uuid)
    .execute(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "steering_evaluations");
    assert_eq!(payload["op"], "UPDATE");
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    sqlx::query("DELETE FROM sessions.sessions WHERE id = $1")
        .bind(session_pk)
        .execute(&pool)
        .await?;
    let payload = recv_matching(&mut listener, "steering_evaluations").await?;
    assert_eq!(payload["op"], "DELETE");
    assert_eq!(payload["id"], eval_uuid.to_string());
    assert!(
        payload.get("session_id").is_none_or(Value::is_null),
        "a same-statement cascade delete of the parent session must route global; payload was {payload}"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
