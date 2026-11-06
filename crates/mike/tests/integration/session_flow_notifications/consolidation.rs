use super::fixtures::{
    assert_no_more, open_listener, recv_matching, recv_payload, seed_project_and_session,
};

use crate::test_db;

use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn consolidation_actions_uses_denormalized_session_uuid() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (project_id, _session_pk, session_id) = seed_project_and_session(&pool).await?;
    let now = chrono::Utc::now();

    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at)
         SELECT k.id, t.id, s.id, $1
           FROM memory.consolidation_run_kinds k, memory.consolidation_triggers t, memory.consolidation_run_statuses s
          WHERE k.code = 'session' AND t.code = 'manual' AND s.code = 'running'
         RETURNING id",
    )
    .bind(now)
    .fetch_one(&pool)
    .await?;

    let (lesson_id, lesson_uuid): (i64, uuid::Uuid) = sqlx::query_as(
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

    let action_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_actions
            (run_id, lesson_id, lesson_uuid, project_id, session_uuid, lesson_text,
             action_kind_id, outcome_id, runners_up)
         SELECT $1, $2, $3, $4, $5::text::uuid, 'test lesson', ak.id, oc.id, '[]'::jsonb
           FROM memory.consolidation_action_kinds ak, memory.consolidation_outcomes oc
          WHERE ak.code = 'promoted_to_long_term' AND oc.code = 'applied'
         RETURNING id",
    )
    .bind(run_id)
    .bind(lesson_id)
    .bind(lesson_uuid)
    .bind(project_id)
    .bind(&session_id)
    .fetch_one(&pool)
    .await?;

    let payload = recv_matching(&mut listener, "consolidation_actions").await?;
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], action_id.to_string());
    assert_eq!(payload["session_id"], session_id);

    let global_action_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_actions
            (run_id, lesson_id, lesson_uuid, project_id, session_uuid, lesson_text,
             action_kind_id, outcome_id, runners_up)
         SELECT $1, $2, $3, $4, NULL, 'test lesson', ak.id, oc.id, '[]'::jsonb
           FROM memory.consolidation_action_kinds ak, memory.consolidation_outcomes oc
          WHERE ak.code = 'promoted_to_long_term' AND oc.code = 'applied'
         RETURNING id",
    )
    .bind(run_id)
    .bind(lesson_id)
    .bind(lesson_uuid)
    .bind(project_id)
    .fetch_one(&pool)
    .await?;
    let payload = recv_matching(&mut listener, "consolidation_actions").await?;
    assert_eq!(payload["id"], global_action_id.to_string());
    assert!(payload.get("session_id").is_none_or(Value::is_null));

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn consolidation_runs_always_omits_session_id() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let mut listener = open_listener(&handle.database_url).await?;
    let now = chrono::Utc::now();

    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at)
         SELECT k.id, t.id, s.id, $1
           FROM memory.consolidation_run_kinds k, memory.consolidation_triggers t, memory.consolidation_run_statuses s
          WHERE k.code = 'session' AND t.code = 'manual' AND s.code = 'running'
         RETURNING id",
    )
    .bind(now)
    .fetch_one(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "consolidation_runs");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], run_id.to_string());
    assert!(payload.get("session_id").is_none());
    assert_no_more(&mut listener).await?;

    sqlx::query(
        "UPDATE memory.consolidation_runs
            SET status_id = (SELECT id FROM memory.consolidation_run_statuses WHERE code = 'completed'),
                finished_at = $2
          WHERE id = $1",
    )
    .bind(run_id)
    .bind(now)
    .execute(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["op"], "UPDATE");
    assert!(payload.get("session_id").is_none());
    assert_no_more(&mut listener).await?;

    sqlx::query("DELETE FROM memory.consolidation_runs WHERE id = $1")
        .bind(run_id)
        .execute(&pool)
        .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["op"], "DELETE");
    assert_eq!(payload["id"], run_id.to_string());
    assert!(payload.get("session_id").is_none());
    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
