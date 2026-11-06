use sqlx::PgPool;

use super::materialize_turn_aggregates;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support as test_util;

const SUGGESTION: &str = "switch it off, local only for now";

async fn seed_session(pool: &PgPool) -> anyhow::Result<(i64, i64)> {
    let project_id = test_util::seed_project(pool, "/tmp/turn-text-fixture").await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_id))
}

async fn seed_request(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    origin: &str,
    text: &str,
) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming, message_count,
             is_tool_loop, status_id, params, last_user_message, human_prompt,
             turn_type_id, turn_origin_id, created_at, completed_at)
         SELECT $1, $2, rm.id, '/v1/messages', true, 1, false, rs.id, '{}'::jsonb, '', '',
                rtt.id, rto.id, NOW(), NOW()
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = $3
          RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(origin)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO responses.stream_blocks
             (request_id, session_id, seq_ord, content, byte_size, content_sha256, block_type_id)
         SELECT $1, $2, 0, $3, octet_length($3), encode(sha256(convert_to($3, 'UTF8')), 'hex'), bt.id
           FROM responses.stream_block_types bt WHERE bt.code = 'text'",
    )
    .bind(request_id)
    .bind(session_id)
    .bind(text)
    .execute(pool)
    .await?;
    Ok(request_id)
}

async fn turn_over(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    requests: &[i64],
) -> anyhow::Result<i64> {
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id, outcome_id, closed_at)
         SELECT $1, $2, $3, $4, tot.id, NOW() FROM memory.turn_outcomes tot WHERE tot.code = 'unknown'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(requests[0])
    .bind(requests[requests.len() - 1])
    .fetch_one(pool)
    .await?;
    sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id = ANY($2)")
        .bind(turn_id)
        .bind(requests)
        .execute(pool)
        .await?;
    Ok(turn_id)
}

async fn materialized_text(handle: &PersistHandle, turn_id: i64) -> anyhow::Result<Option<String>> {
    let mut tx = handle.pool.begin().await?;
    materialize_turn_aggregates(&mut tx, turn_id).await?;
    tx.commit().await?;
    Ok(
        sqlx::query_scalar("SELECT final_assistant_text FROM memory.operator_turns WHERE id = $1")
            .bind(turn_id)
            .fetch_one(&handle.pool)
            .await?,
    )
}

#[tokio::test]
async fn final_text_leaves_out_suggestion_sidecar_text() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (project_id, session_id) = seed_session(&handle.pool).await?;
    let answer = seed_request(&handle.pool, session_id, project_id, "primary", "All done.").await?;
    let suggestion = seed_request(
        &handle.pool,
        session_id,
        project_id,
        "next_input_suggestion_sidecar",
        SUGGESTION,
    )
    .await?;
    let turn = turn_over(&handle.pool, session_id, project_id, &[answer, suggestion]).await?;

    let text = materialized_text(&handle, turn).await?;

    assert_eq!(text.as_deref(), Some("All done."));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn final_text_is_empty_when_only_a_sidecar_spoke() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (project_id, session_id) = seed_session(&handle.pool).await?;
    let suggestion = seed_request(
        &handle.pool,
        session_id,
        project_id,
        "next_input_suggestion_sidecar",
        SUGGESTION,
    )
    .await?;
    let turn = turn_over(&handle.pool, session_id, project_id, &[suggestion]).await?;

    let text = materialized_text(&handle, turn).await?;

    assert_eq!(text, None);
    db.cleanup().await?;
    Ok(())
}
