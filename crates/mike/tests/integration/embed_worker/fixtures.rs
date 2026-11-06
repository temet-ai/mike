use chrono::DateTime;
use chrono::Utc;
use sqlx::PgPool;

pub(super) const EMBED_WORKER_SELECT: &str = "
    SELECT t.id AS turn_id,
           COALESCE(v.user_prompt, '')          AS user_prompt,
           COALESCE(v.final_assistant_text, '') AS final_assistant_text
      FROM memory.operator_turns t
      LEFT JOIN mike.operator_turn_text_v v ON v.turn_id = t.id
     WHERE t.closed_at   IS NOT NULL
       AND t.embedded_at IS NULL
     ORDER BY t.closed_at ASC
     LIMIT $1
     FOR UPDATE OF t SKIP LOCKED
";

pub(super) async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let now = Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         ON CONFLICT (project_path) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_pk))
}

pub(super) async fn insert_anchor_request(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    created_at: DateTime<Utc>,
    last_user_message: &str,
) -> anyhow::Result<(i64, DateTime<Utc>)> {
    let row: (i64, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            '{}'::jsonb,
            $4, '',
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $3
         ) RETURNING id, created_at",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(created_at)
    .bind(last_user_message)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub(super) struct TurnSpec {
    pub(super) session_id: i64,
    pub(super) project_id: i64,
    pub(super) start_request_id: i64,
    pub(super) closed_at: Option<DateTime<Utc>>,
    pub(super) embedded_at: Option<DateTime<Utc>>,
}

pub(super) async fn insert_turn(pool: &PgPool, spec: TurnSpec) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            closed_at, embedded_at
         ) VALUES ($1, $2, $3, $3, $4, $5)
         RETURNING id",
    )
    .bind(spec.session_id)
    .bind(spec.project_id)
    .bind(spec.start_request_id)
    .bind(spec.closed_at)
    .bind(spec.embedded_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
