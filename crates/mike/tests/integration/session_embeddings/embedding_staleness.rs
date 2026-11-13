use crate::test_db;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use super::fixtures::{
    make_pool_and_persist, one_hot, seed_chunk, seed_project, seed_session, seed_turn,
    set_session_overview,
};
use chrono::{Duration as ChronoDuration, SubsecRound, Utc};
use mike::store::persistence::session_embeddings::SessionEmbeddingWrite;
use test_db::TestDb;
use uuid::Uuid;

#[tokio::test]
async fn list_sessions_needing_embedding_detects_missing_and_stale_sessions() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping \
             list_sessions_needing_embedding_detects_missing_and_stale_sessions"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(&pool, "list-needing-embedding-test").await?;
    let now = Utc::now().trunc_subsecs(6);

    let (m_pk, m_uuid) = seed_session(&pool, project_id, Some("M"), now).await?;
    let m_turn = seed_turn(&pool, m_pk, project_id).await?;
    let t_m = now - ChronoDuration::seconds(300);
    seed_chunk(&pool, m_turn, 0, &one_hot(0, 1.0), t_m).await?;

    let (s_pk, s_uuid) = seed_session(&pool, project_id, Some("S"), now).await?;
    let s_turn = seed_turn(&pool, s_pk, project_id).await?;
    let t_s1 = now - ChronoDuration::seconds(295);
    seed_chunk(&pool, s_turn, 0, &one_hot(1, 1.0), t_s1).await?;
    let knowledge_db = persist.knowledge();
    knowledge_db
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: s_uuid,
            embedding: &one_hot(1, 1.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: t_s1,
        })
        .await?;
    let t_s2 = now - ChronoDuration::seconds(200);
    seed_chunk(&pool, s_turn, 1, &one_hot(1, 1.0), t_s2).await?;

    let (o_pk, o_uuid) = seed_session(&pool, project_id, Some("O"), now).await?;
    let o_turn = seed_turn(&pool, o_pk, project_id).await?;
    let t_o1 = now - ChronoDuration::seconds(290);
    seed_chunk(&pool, o_turn, 0, &one_hot(2, 1.0), t_o1).await?;
    knowledge_db
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: o_uuid,
            embedding: &one_hot(2, 1.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: t_o1,
        })
        .await?;
    let t_o_overview = now - ChronoDuration::seconds(100);
    set_session_overview(&pool, o_pk, "stale overview text", t_o_overview).await?;

    let (u_pk, u_uuid) = seed_session(&pool, project_id, Some("U"), now).await?;
    let u_turn = seed_turn(&pool, u_pk, project_id).await?;
    let t_u1 = now - ChronoDuration::seconds(290);
    seed_chunk(&pool, u_turn, 0, &one_hot(3, 1.0), t_u1).await?;
    knowledge_db
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: u_uuid,
            embedding: &one_hot(3, 1.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;

    let (z_pk, _z_uuid) = seed_session(&pool, project_id, Some("Z"), now).await?;
    set_session_overview(&pool, z_pk, "z has no chunks yet", now).await?;

    let all = knowledge_db.list_sessions_needing_embedding(10).await?;
    let ids: Vec<Uuid> = all.iter().map(|c| c.session_id).collect();
    assert_eq!(
        ids,
        vec![m_uuid, s_uuid, o_uuid],
        "must return exactly the missing/stale sessions, ordered oldest-signal-first \
         (M's chunk-only signal at -300s, S's newer-chunk signal at -200s, O's \
         overview signal at -100s), excluding the up-to-date session U and the \
         chunk-less session Z"
    );

    let m_candidate = &all[0];
    assert_eq!(m_candidate.session_id, m_uuid);
    assert!(
        m_candidate.overview.is_none(),
        "M has never had an overview written"
    );
    assert!(m_candidate.overview_generated_at.is_none());
    assert_eq!(m_candidate.max_chunk_embedded_at, Some(t_m));

    let o_candidate = &all[2];
    assert_eq!(o_candidate.session_id, o_uuid);
    assert_eq!(o_candidate.overview.as_deref(), Some("stale overview text"));
    assert_eq!(o_candidate.overview_generated_at, Some(t_o_overview));
    assert_eq!(
        o_candidate.max_chunk_embedded_at,
        Some(t_o1),
        "O's chunk signal stays its own chunk's embedded_at even though the \
         overview signal is what makes it stale"
    );

    let limited = knowledge_db.list_sessions_needing_embedding(2).await?;
    let limited_ids: Vec<Uuid> = limited.iter().map(|c| c.session_id).collect();
    assert_eq!(limited_ids, vec![m_uuid, s_uuid]);

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
