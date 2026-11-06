use super::fixtures::{
    assert_no_more, insert_request, open_listener, recv_matching, recv_payload,
    seed_project_and_session,
};

use crate::test_db;

use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn lesson_injections_routes_session_then_request_then_global() -> anyhow::Result<()> {
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
    let request_id = insert_request(&pool, project_id, session_pk).await?;

    let (lesson_id, _lesson_uuid): (i64, uuid::Uuid) = sqlx::query_as(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, 'test lesson', ls.id, lsrc.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'convention' AND ls.code = 'long_term' AND lsrc.code = 'operator'
         RETURNING id, uuid",
    )
    .bind(project_id)
    .fetch_one(&pool)
    .await?;

    let mut listener = open_listener(&handle.database_url).await?;

    let injection_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lesson_injections (lesson_id, request_id, session_id, lane_id)
         SELECT $1, $2, $3, lane.id
           FROM memory.lesson_injection_lanes lane
          WHERE lane.code = 'primary'
         RETURNING id",
    )
    .bind(lesson_id)
    .bind(request_id)
    .bind(session_pk)
    .fetch_one(&pool)
    .await?;

    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "lesson_injections");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], injection_id.to_string());
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    sqlx::query("UPDATE memory.lesson_injections SET session_id = NULL WHERE id = $1")
        .bind(injection_id)
        .execute(&pool)
        .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "lesson_injections");
    assert_eq!(payload["op"], "UPDATE");
    assert_eq!(
        payload["session_id"], session_id,
        "with session_id NULL, the trigger must resolve session via request_id"
    );
    assert_no_more(&mut listener).await?;

    sqlx::query("DELETE FROM requests.requests WHERE id = $1")
        .bind(request_id)
        .execute(&pool)
        .await?;
    let payload = recv_matching(&mut listener, "lesson_injections").await?;
    assert_eq!(payload["op"], "DELETE");
    assert_eq!(payload["id"], injection_id.to_string());
    assert!(
        payload.get("session_id").is_none_or(Value::is_null),
        "with both session_id and the parent request gone, routing must fall back to global; payload was {payload}"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
