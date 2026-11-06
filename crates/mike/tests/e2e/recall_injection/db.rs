use sqlx::PgPool;

pub(super) async fn insert_lesson_with_topic(
    pool: &PgPool,
    project_id: i64,
    kind: &str,
    text: &str,
    topic: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, $3
         FROM memory.lesson_kinds lk,
              memory.lesson_stages ls,
              memory.lesson_sources lsrc
         WHERE lk.code = $4
           AND ls.code = 'daily'
           AND lsrc.code = 'operator'",
    )
    .bind(project_id)
    .bind(text)
    .bind(topic)
    .bind(kind)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn insert_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn project_id_for_path(
    pool: &PgPool,
    project_path: &str,
) -> anyhow::Result<Option<i64>> {
    let id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(project_path)
            .fetch_optional(pool)
            .await?;
    Ok(id)
}
