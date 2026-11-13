use crate::test_db;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use super::fixtures::{
    make_pool_and_persist, planar_unit, seed_chunk, seed_project, seed_session, seed_turn,
};
use anyhow::Context;
use chrono::{Duration as ChronoDuration, SubsecRound, Utc};
use test_db::TestDb;

#[tokio::test]
async fn session_chunk_centroid_is_normalised_mean_of_two_chunks() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_chunk_centroid_is_normalised_mean");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id =
        seed_project(&pool, &format!("centroid-test-{}", uuid::Uuid::new_v4())).await?;
    let now = Utc::now().trunc_subsecs(6);
    let (session_pk, session_uuid) = seed_session(&pool, project_id, None, now).await?;
    let turn_id = seed_turn(&pool, session_pk, project_id).await?;

    let v1 = planar_unit(0.0);
    let v2 = planar_unit(90.0);
    let t1 = now - ChronoDuration::seconds(10);
    let t2 = now - ChronoDuration::seconds(5);
    seed_chunk(&pool, turn_id, 0, &v1, t1).await?;
    seed_chunk(&pool, turn_id, 1, &v2, t2).await?;

    let (centroid, chunk_count, max_embedded_at) = persist
        .knowledge()
        .session_chunk_centroid(session_uuid)
        .await?
        .context("session has two embedded chunks; centroid must be Some")?;

    assert_eq!(chunk_count, 2);
    assert_eq!(
        max_embedded_at, t2,
        "max embedded_at must be the later chunk's timestamp"
    );

    let expected = 1.0_f32 / 2.0_f32.sqrt();
    let eps = 1e-4_f32;
    assert!(
        (centroid[0] - expected).abs() < eps,
        "centroid[0] = {} must be within {eps} of {expected}",
        centroid[0]
    );
    assert!(
        (centroid[1] - expected).abs() < eps,
        "centroid[1] = {} must be within {eps} of {expected}",
        centroid[1]
    );
    for (i, x) in centroid.iter().enumerate().skip(2) {
        assert!(x.abs() < eps, "centroid[{i}] = {x} must be ~0");
    }

    let (_, empty_uuid) = seed_session(&pool, project_id, None, now).await?;
    assert!(
        persist
            .knowledge()
            .session_chunk_centroid(empty_uuid)
            .await?
            .is_none()
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
