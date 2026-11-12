use crate::test_db;

use super::fixtures::{make_pool_and_persist, planar_unit, seed_project, seed_session};
use anyhow::Context;
use chrono::{SubsecRound, Utc};
use mike::store::persistence::session_embeddings::SessionEmbeddingWrite;
use test_db::TestDb;
use uuid::Uuid;

#[tokio::test]
async fn nearest_sessions_orders_excludes_self_and_filters() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping nearest_sessions_orders_excludes_self_and_filters"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_p1 = seed_project(&pool, "nearest-p1").await?;
    let project_p2 = seed_project(&pool, "nearest-p2").await?;
    let now = Utc::now().trunc_subsecs(6);

    let (_a_pk, a_uuid) = seed_session(&pool, project_p1, Some("A"), now).await?;
    let (_b_pk, b_uuid) = seed_session(&pool, project_p1, Some("B"), now).await?;
    let (_c_pk, c_uuid) = seed_session(&pool, project_p1, Some("C"), now).await?;
    let (_d_pk, d_uuid) = seed_session(&pool, project_p2, Some("D"), now).await?;

    sqlx::query("UPDATE sessions.sessions SET last_status = 'Complete' WHERE session_uuid = $1")
        .bind(b_uuid)
        .execute(&pool)
        .await?;

    for (uuid, angle) in [
        (a_uuid, 0.0),
        (b_uuid, 10.0),
        (c_uuid, 80.0),
        (d_uuid, 30.0),
    ] {
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

    let all = persist.nearest_sessions(a_uuid, 10, false, 0.0).await?;
    let ids: Vec<Uuid> = all.iter().map(|n| n.session_id).collect();
    assert!(
        !ids.contains(&a_uuid),
        "the anchor session must never appear in its own neighbours"
    );
    assert_eq!(
        ids,
        vec![b_uuid, d_uuid, c_uuid],
        "must be ordered most-similar-first: B (~0.98) > D (~0.87) > C (~0.17)"
    );
    let b_node = all
        .iter()
        .find(|n| n.session_id == b_uuid)
        .context("B present in neighbours")?;
    assert_eq!(
        b_node.last_status.as_deref(),
        Some("Complete"),
        "last_status must surface the sessions.sessions cache column"
    );
    let d_node = all
        .iter()
        .find(|n| n.session_id == d_uuid)
        .context("D present in neighbours")?;
    assert_eq!(
        d_node.last_status, None,
        "a session with no cached status and no request rows must come back None, not a bogus default"
    );

    let same_project = persist.nearest_sessions(a_uuid, 10, true, 0.0).await?;
    let same_project_ids: Vec<Uuid> = same_project.iter().map(|n| n.session_id).collect();
    assert_eq!(same_project_ids, vec![b_uuid, c_uuid]);

    let filtered = persist.nearest_sessions(a_uuid, 10, false, 0.5).await?;
    let filtered_ids: Vec<Uuid> = filtered.iter().map(|n| n.session_id).collect();
    assert_eq!(filtered_ids, vec![b_uuid, d_uuid]);

    let capped = persist.nearest_sessions(a_uuid, 1, false, 0.0).await?;
    assert_eq!(capped.len(), 1);
    assert_eq!(capped[0].session_id, b_uuid);

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
