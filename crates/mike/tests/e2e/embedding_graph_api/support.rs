use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

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
    title: Option<&str>,
) -> anyhow::Result<(i64, Uuid)> {
    let session_uuid = Uuid::new_v4();
    let now = Utc::now();
    let pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, title, first_seen, last_seen)
         VALUES ($1, $2, $3, $4, $4)
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(title)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok((pk, session_uuid))
}

pub(super) fn planar_unit(angle_deg: f32) -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    let rad = angle_deg.to_radians();
    v[0] = rad.cos();
    v[1] = rad.sin();
    v
}

pub(super) async fn set_narrative_overview(
    pool: &PgPool,
    session_pk: i64,
    text: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE sessions.sessions SET overview = $2, overview_generated_at = $3 WHERE id = $1",
    )
    .bind(session_pk)
    .bind(text)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}
