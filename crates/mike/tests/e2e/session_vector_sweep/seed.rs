use chrono::Utc;
use sqlx::PgPool;

use super::fixtures::vec_literal;

pub(super) async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_session(
    pool: &PgPool,
    project_id: i64,
) -> anyhow::Result<(i64, uuid::Uuid)> {
    let session_uuid = uuid::Uuid::new_v4();
    let now = Utc::now();
    let pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, $3, $3)
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok((pk, session_uuid))
}

pub(super) async fn seed_closed_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
) -> anyhow::Result<i64> {
    let now = Utc::now();
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            '{}'::jsonb, '', '',
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $3
         ) RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id, closed_at)
         VALUES ($1, $2, $3, $3, $4)
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(request_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(turn_id)
}

pub(super) async fn seed_chunk(pool: &PgPool, turn_id: i64, chunk_idx: i32) -> anyhow::Result<()> {
    let v = vec![0.1_f32; 768];
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding,
             embedding_model, embedded_at)
         VALUES ($1, $2, (SELECT id FROM memory.chunk_roles WHERE code = 'prompt'),
                 'seed chunk text', $3::vector, 'test-model', $4)",
    )
    .bind(turn_id)
    .bind(chunk_idx)
    .bind(vec_literal(&v))
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn fetch_embedding_row(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<Option<(String, i32)>> {
    let row: Option<(String, i32)> = sqlx::query_as(
        "SELECT source, chunk_count FROM memory.session_embeddings WHERE session_id = $1",
    )
    .bind(session_uuid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
