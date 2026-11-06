use crate::test_db;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use test_db::TestDb;

#[tokio::test]
async fn lesson_dedup() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_path = "dedup-proj";
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(&pool)
    .await?;

    let mut e0 = vec![0.0_f32; 768];
    e0[0] = 1.0;
    let e0_literal = format!(
        "[{}]",
        e0.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let _neighbour_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO memory.lessons
            (project_id, kind_id, text, stage_id, source_id, embedding, embedding_model, embedded_at)
         SELECT $1, lk.id, 'existing neighbour', ls.id, lsrc.id, $2::vector, 'bge-small-en-v1.5', NOW()
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'convention' AND ls.code = 'daily' AND lsrc.code = 'operator'
         RETURNING uuid",
    )
    .bind(project_id)
    .bind(&e0_literal)
    .fetch_one(&pool)
    .await?;

    let mut near = vec![0.0_f32; 768];
    near[0] = 0.999;
    near[1] = 0.0447;
    let mut far = vec![0.0_f32; 768];
    far[5] = 1.0;

    let no_exclude = uuid::Uuid::nil();
    let dist_near = nearest_distance(&pool, project_id, &near, no_exclude).await?;
    let dist_far = nearest_distance(&pool, project_id, &far, no_exclude).await?;

    let distance_threshold = 1.0_f32 - 0.85_f32;
    assert!(
        dist_near < distance_threshold,
        "near vector must be a duplicate (dist {dist_near} < {distance_threshold})"
    );
    assert!(
        dist_far > distance_threshold,
        "orthogonal vector must NOT be a duplicate (dist {dist_far} > {distance_threshold})"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn nearest_distance(
    pool: &PgPool,
    project_id: i64,
    query: &[f32],
    exclude_id: uuid::Uuid,
) -> anyhow::Result<f32> {
    let literal = format!(
        "[{}]",
        query
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let dist: f32 = sqlx::query_scalar(
        "SELECT (l.embedding <=> $2::vector)::float4 AS distance
           FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.project_id = $1
            AND ls.code IN ('daily', 'long_term')
            AND l.embedding IS NOT NULL
            AND l.uuid <> $3
          ORDER BY l.embedding <=> $2::vector
          LIMIT 1",
    )
    .bind(project_id)
    .bind(&literal)
    .bind(exclude_id)
    .fetch_one(pool)
    .await?;
    Ok(dist)
}
