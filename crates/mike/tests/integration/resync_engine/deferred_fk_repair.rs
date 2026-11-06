use crate::test_db;

use super::seed::seed_source;
use super::support::{BASELINE_TABLES, baseline_cursors, connect, count};
use mike::resync::ResyncEngine;
use test_db::TestDb;

#[tokio::test]
async fn replay_pending_survives_deferred_fk_target_deleted_on_dest() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine deferred-fk-delete test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    baseline_cursors(&source, BASELINE_TABLES).await?;
    seed_source(&source).await?;

    let engine = ResyncEngine::new(source.clone(), dest.clone());
    engine.replay_pending().await?;
    let stamped_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM requests.requests WHERE operator_turn_id IS NOT NULL",
    )
    .fetch_one(&dest)
    .await?;
    assert_eq!(stamped_before, 2, "first replay must stamp both requests");

    sqlx::query("DELETE FROM memory.operator_turns")
        .execute(&dest)
        .await?;
    let stamped_after_delete: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM requests.requests WHERE operator_turn_id IS NOT NULL",
    )
    .fetch_one(&dest)
    .await?;
    assert_eq!(
        stamped_after_delete, 0,
        "precondition: the dest delete must cascade-null both requests"
    );

    let (session_id, project_id, turn_id): (i64, i64, i64) = sqlx::query_as(
        "SELECT r.session_id, r.project_id, r.operator_turn_id
           FROM requests.requests r
          WHERE r.operator_turn_id IS NOT NULL LIMIT 1",
    )
    .fetch_one(&source)
    .await?;
    sqlx::query(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, model, is_streaming,
             message_count, is_tool_loop, status_id, params, last_user_message,
             human_prompt, turn_type_id, turn_origin_id, operator_turn_id)
         VALUES ($1, $2, 2, '/v1/messages', 'claude-x', true, 3, true, 3,
                 '{}'::jsonb, 'hi', 'hi', 2, 1, $3)",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(turn_id)
    .execute(&source)
    .await?;

    let report2 = engine.replay_pending().await?;
    eprintln!("post-delete replay report: {report2:?}");

    assert_eq!(
        count(&dest, "requests.requests").await?,
        3,
        "the new request must still insert on dest"
    );
    let stamped_after_second_replay: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM requests.requests WHERE operator_turn_id IS NOT NULL",
    )
    .fetch_one(&dest)
    .await?;
    assert_eq!(
        stamped_after_second_replay, 0,
        "no dest row may be set to a target that no longer exists"
    );
    assert_eq!(
        count(&dest, "memory.operator_turns").await?,
        0,
        "the deleted operator_turn must not be silently recreated"
    );

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}
