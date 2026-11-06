use mike::config::AppState;
use mike::config::ProxyConfig;

fn session_id_for_request(request_id: i64) -> String {
    format!("f47ac10b-58cc-4372-a567-{:012x}", request_id)
}

pub(super) const DEFINITELY_STALE_AGE: chrono::Duration = chrono::Duration::hours(2);

pub(super) async fn boot_state(database_url: &str) -> anyhow::Result<AppState> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    let state = AppState::new(config).await?;
    Ok(state)
}

pub(super) async fn insert_pending_request(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> anyhow::Result<()> {
    insert_pending_request_aged(pool, request_id, chrono::Duration::zero()).await
}

pub(super) async fn insert_pending_request_aged(
    pool: &sqlx::PgPool,
    request_id: i64,
    age: chrono::Duration,
) -> anyhow::Result<()> {
    let session_id = session_id_for_request(request_id);
    let created_at = chrono::Utc::now() - age;

    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects
                 (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, NOW(), NOW()
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects.projects WHERE project_path = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind("/test/crash-recovery")
    .fetch_one(pool)
    .await?;

    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(&session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.requests
            (id, session_id, project_id, method_id, path, status_id,
             turn_type_id, turn_origin_id, is_streaming, message_count,
             is_tool_loop, params,
             last_user_message, human_prompt,
             created_at)
         OVERRIDING SYSTEM VALUE
         SELECT $1, $2, $3, rm.id, '/v1/messages', rs.id,
                rtt.id, rto.id, TRUE, 1,
                FALSE, '{}'::jsonb,
                'Hello', 'Hello',
                $4
         FROM requests.methods rm,
              requests.statuses rs,
              requests.turn_types rtt,
              requests.turn_origins rto
         WHERE rm.code  = 'POST'
           AND rs.code  = 'pending'
           AND rtt.code = 'user_prompt'
           AND rto.code = 'primary'",
    )
    .bind(request_id)
    .bind(session_pk)
    .bind(project_id)
    .bind(created_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn fetch_status(pool: &sqlx::PgPool, request_id: i64) -> anyhow::Result<String> {
    let row: (String,) = sqlx::query_as(
        "SELECT CASE st.code
                    WHEN 'pending'   THEN 'Pending'
                    WHEN 'streaming' THEN 'Streaming'
                    WHEN 'complete'  THEN 'Complete'
                    WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
                END
         FROM requests.requests r
         JOIN requests.statuses st ON st.id = r.status_id
         LEFT JOIN requests.errors re ON re.request_id = r.id
         WHERE r.id = $1",
    )
    .bind(request_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub(super) async fn set_session_last_status(
    pool: &sqlx::PgPool,
    request_id: i64,
    status: &str,
) -> anyhow::Result<()> {
    let session_id = session_id_for_request(request_id);
    sqlx::query(
        "UPDATE sessions.sessions SET last_status = $2 WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn fetch_session_last_status(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> anyhow::Result<Option<String>> {
    let session_id = session_id_for_request(request_id);
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT last_status FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
