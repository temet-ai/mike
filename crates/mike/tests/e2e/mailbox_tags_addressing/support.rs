use sqlx::PgPool;

pub(super) async fn ensure_session(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
) -> anyhow::Result<(i64, i64)> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         ON CONFLICT (project_path)
         DO UPDATE SET last_seen = GREATEST(projects.projects.last_seen, EXCLUDED.last_seen)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1::text::uuid, $2, $3, $3)
             ON CONFLICT (session_uuid) DO NOTHING
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid
         LIMIT 1",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok((id, project_id))
}

pub(super) async fn seed_live_registration(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at)
         VALUES ($1, $2, '/tmp/mailbox-tags-addressing', 'rest', now() + interval '15 minutes')",
    )
    .bind(session_id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn seed_role(pool: &PgPool, session_id: i64, role: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO sessions.session_goals (session_id, goal, role)
         VALUES ($1, 'test goal', $2)
         ON CONFLICT (session_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(session_id)
    .bind(role)
    .execute(pool)
    .await?;
    Ok(())
}
