use crate::test_db;

use super::fixtures::{make_pool_and_persist, planar_unit, seed_project, seed_session};
use chrono::{DateTime, Duration as ChronoDuration, SubsecRound, Utc};
use mike::store::persistence::SessionEmbeddingWrite;
use test_db::TestDb;

#[tokio::test]
async fn refresh_project_centroids_writes_and_skips_up_to_date() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping refresh_project_centroids_writes_and_skips_up_to_date"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_a = seed_project(&pool, "refresh-a").await?;
    let project_b = seed_project(&pool, "refresh-b").await?;
    let now = Utc::now().trunc_subsecs(6);

    let (_p1, u1) = seed_session(&pool, project_a, None, now).await?;
    let (_p2, u2) = seed_session(&pool, project_a, None, now).await?;
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: u1,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: u2,
            embedding: &planar_unit(20.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;

    let (_p3, u3) = seed_session(&pool, project_b, None, now).await?;
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: u3,
            embedding: &planar_unit(45.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;

    let written_first = persist.refresh_project_centroids().await?;
    assert_eq!(
        written_first, 2,
        "both projects have session vectors and no prior centroid row"
    );

    let status_a = persist.project_centroid_status(project_a).await?;
    assert!(status_a.present);
    assert_eq!(status_a.session_count, Some(2));
    let status_b = persist.project_centroid_status(project_b).await?;
    assert!(status_b.present);
    assert_eq!(status_b.session_count, Some(1));

    let written_second = persist.refresh_project_centroids().await?;
    assert_eq!(written_second, 0, "up-to-date projects must be skipped");

    let (_p4, u4) = seed_session(&pool, project_a, None, now).await?;
    let later = now + ChronoDuration::seconds(60);
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: u4,
            embedding: &planar_unit(200.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: later,
        })
        .await?;
    let new_row_embedded_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT embedded_at FROM memory.session_embeddings WHERE session_id = $1",
    )
    .bind(u4)
    .fetch_one(&pool)
    .await?;
    let project_a_embedded_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT embedded_at FROM memory.project_embeddings WHERE project_id = $1",
    )
    .bind(project_a)
    .fetch_one(&pool)
    .await?;
    assert!(new_row_embedded_at > project_a_embedded_at);

    let written_third = persist.refresh_project_centroids().await?;
    assert_eq!(
        written_third, 1,
        "only project_a has a newer session vector"
    );
    let status_a2 = persist.project_centroid_status(project_a).await?;
    assert_eq!(status_a2.session_count, Some(3));

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
