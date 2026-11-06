use sqlx::PgPool;

use serde_json::Value;

pub(super) async fn ensure_session(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
) -> anyhow::Result<()> {
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

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) fn context_items_for<'a>(rows: &'a [Value], message_uuid: &str) -> Option<&'a Value> {
    rows.iter()
        .find(|r| r["message_uuid"].as_str() == Some(message_uuid))
        .map(|r| &r["context_items"])
}
