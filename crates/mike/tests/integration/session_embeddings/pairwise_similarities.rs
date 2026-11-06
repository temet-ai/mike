use crate::test_db;

use super::fixtures::{make_pool_and_persist, planar_unit, seed_project, seed_session};
use chrono::{SubsecRound, Utc};
use mike::store::persistence::SessionEmbeddingWrite;
use test_db::TestDb;

#[tokio::test]
async fn pairwise_session_similarities_returns_only_pairs_with_both_vectors() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping \
             pairwise_session_similarities_returns_only_pairs_with_both_vectors"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id =
        seed_project(&pool, &format!("pairwise-test-{}", uuid::Uuid::new_v4())).await?;
    let now = Utc::now().trunc_subsecs(6);

    let (_a_pk, a_uuid) = seed_session(&pool, project_id, Some("A"), now).await?;
    let (_b_pk, b_uuid) = seed_session(&pool, project_id, Some("B"), now).await?;
    let (_c_pk, c_uuid) = seed_session(&pool, project_id, Some("C"), now).await?;

    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: a_uuid,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(10.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;

    let pairs = persist
        .pairwise_session_similarities(&[a_uuid, b_uuid, c_uuid])
        .await?;
    assert_eq!(
        pairs.len(),
        1,
        "only the (A, B) pair has a vector on both sides; every pair touching C is absent"
    );
    let (lo, hi) = if a_uuid < b_uuid {
        (a_uuid, b_uuid)
    } else {
        (b_uuid, a_uuid)
    };
    assert_eq!((pairs[0].0, pairs[0].1), (lo, hi), "must report a < b");
    let eps = 1e-3;
    assert!(
        (pairs[0].2 - (10.0_f64.to_radians()).cos()).abs() < eps,
        "similarity {} must match cos(10deg)",
        pairs[0].2
    );

    let (_none_pk, none_uuid) = seed_session(&pool, project_id, None, now).await?;
    let empty = persist
        .pairwise_session_similarities(&[c_uuid, none_uuid])
        .await?;
    assert!(empty.is_empty());

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
