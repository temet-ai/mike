use chrono::{DateTime, SubsecRound, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;

pub(super) fn vec_literal(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 12 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{x}"));
    }
    s.push(']');
    s
}

pub(super) fn one_hot(dim: usize, value: f32) -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    v[dim] = value;
    v
}

pub(super) fn planar_unit(angle_deg: f32) -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    let rad = angle_deg.to_radians();
    v[0] = rad.cos();
    v[1] = rad.sin();
    v
}

pub(super) async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let now = Utc::now().trunc_subsecs(6);
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
    title: Option<&str>,
    last_seen: DateTime<Utc>,
) -> anyhow::Result<(i64, uuid::Uuid)> {
    let session_uuid = uuid::Uuid::new_v4();
    let now = Utc::now().trunc_subsecs(6);
    let pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, title, first_seen, last_seen)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(title)
    .bind(now)
    .bind(last_seen)
    .fetch_one(pool)
    .await?;
    Ok((pk, session_uuid))
}

pub(super) async fn seed_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
) -> anyhow::Result<i64> {
    let now = Utc::now().trunc_subsecs(6);
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

pub(super) async fn seed_chunk(
    pool: &PgPool,
    turn_id: i64,
    chunk_idx: i32,
    embedding: &[f32],
    embedded_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding,
             embedding_model, embedded_at)
         VALUES ($1, $2, (SELECT id FROM memory.chunk_roles WHERE code = 'prompt'),
                 'seed chunk text', $3::vector, 'test-model', $4)
         RETURNING id",
    )
    .bind(turn_id)
    .bind(chunk_idx)
    .bind(vec_literal(embedding))
    .bind(embedded_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn make_pool_and_persist(
    database_url: &str,
) -> anyhow::Result<(PgPool, PersistHandle)> {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await?;
    let persist = PersistHandle::connect(database_url).await?;
    Ok((pool, persist))
}

pub(super) async fn set_session_overview(
    pool: &PgPool,
    session_pk: i64,
    overview: &str,
    generated_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE sessions.sessions SET overview = $2, overview_generated_at = $3 WHERE id = $1",
    )
    .bind(session_pk)
    .bind(overview)
    .bind(generated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn seed_verdict(pool: &PgPool, session_pk: i64) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO session_judges.verdicts
            (session_id, outcome_id, judge_model, rubric_version)
         VALUES ($1, (SELECT id FROM session_judges.outcomes WHERE code = 'success'),
                 'test-judge', 1)
         RETURNING id",
    )
    .bind(session_pk)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
