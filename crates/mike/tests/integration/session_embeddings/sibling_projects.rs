use crate::test_db;

use super::fixtures::{make_pool_and_persist, planar_unit, seed_project, seed_session};
use chrono::{SubsecRound, Utc};
use mike::store::persistence::session_embeddings::SessionEmbeddingWrite;
use test_db::TestDb;

#[tokio::test]
async fn sibling_projects_orders_by_similarity_excludes_self_and_caps() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping \
             sibling_projects_orders_by_similarity_excludes_self_and_caps"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let anchor_project = seed_project(&pool, "sibling-anchor").await?;
    let near_project = seed_project(&pool, "sibling-near").await?;
    let far_project = seed_project(&pool, "sibling-far").await?;
    let now = Utc::now().trunc_subsecs(6);

    for (project_id, angle) in [
        (anchor_project, 0.0),
        (near_project, 10.0),
        (far_project, 90.0),
    ] {
        let (_pk, uuid) = seed_session(&pool, project_id, None, now).await?;
        persist
            .upsert_session_embedding(SessionEmbeddingWrite {
                session_id: uuid,
                embedding: &planar_unit(angle),
                model: "test-model",
                source: "centroid",
                chunk_count: 1,
                source_updated_at: now,
            })
            .await?;
    }
    let written = persist.refresh_project_centroids().await?;
    assert_eq!(
        written, 3,
        "all three projects qualify for a first centroid"
    );

    let all = persist.sibling_projects(anchor_project, 10).await?;
    let ids: Vec<i64> = all.iter().map(|s| s.project_id).collect();
    assert_eq!(
        ids,
        vec![near_project, far_project],
        "must exclude the anchor project itself and order most-similar-first"
    );
    assert!(
        all[0].similarity > all[1].similarity,
        "near (~0.985) must score higher than far (~0.0)"
    );
    let eps = 1e-3;
    assert!(
        (all[0].similarity - (10.0_f64.to_radians()).cos()).abs() < eps,
        "near's similarity {} must match cos(10deg)",
        all[0].similarity
    );
    assert_eq!(
        all[0].project_path, "sibling-near",
        "project_path must be carried through the join"
    );

    let capped = persist.sibling_projects(anchor_project, 1).await?;
    assert_eq!(capped.len(), 1);
    assert_eq!(capped[0].project_id, near_project);

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
