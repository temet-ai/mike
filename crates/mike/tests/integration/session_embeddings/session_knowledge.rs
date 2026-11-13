use crate::test_db;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use super::fixtures::{make_pool_and_persist, seed_project, seed_session, seed_verdict};
use anyhow::Context;
use chrono::{SubsecRound, Utc};
use test_db::TestDb;
use uuid::Uuid;

#[tokio::test]
async fn session_knowledge_round_trips_upsert_fetch_and_fetch_many() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping \
             session_knowledge_round_trips_upsert_fetch_and_fetch_many"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(
        &pool,
        &format!("session-knowledge-test-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let now = Utc::now().trunc_subsecs(6);
    let (session1_pk, session1_uuid) = seed_session(&pool, project_id, Some("K1"), now).await?;
    let (_session2_pk, session2_uuid) = seed_session(&pool, project_id, Some("K2"), now).await?;

    assert!(
        persist
            .knowledge()
            .fetch_session_knowledge(session1_uuid)
            .await?
            .is_none()
    );

    let verdict_id = seed_verdict(&pool, session1_pk).await?;
    let topics = vec!["auth".to_string(), "retries".to_string()];
    let learnings = vec!["backoff must be jittered".to_string()];
    let related = vec![session2_uuid];
    persist
        .knowledge()
        .upsert_session_knowledge(
            session1_uuid,
            &topics,
            &learnings,
            &related,
            Some(verdict_id),
        )
        .await?;

    let fetched = persist
        .knowledge()
        .fetch_session_knowledge(session1_uuid)
        .await?
        .context("row must exist after upsert")?;
    assert_eq!(fetched.session_id, session1_uuid);
    assert_eq!(fetched.topics, topics);
    assert_eq!(fetched.learnings, learnings);
    assert_eq!(fetched.related_session_ids, related);
    assert_eq!(fetched.verdict_id, Some(verdict_id));

    let topics2 = vec!["logging".to_string()];
    let learnings2: Vec<String> = vec![];
    let related2: Vec<Uuid> = vec![];
    persist
        .knowledge()
        .upsert_session_knowledge(session1_uuid, &topics2, &learnings2, &related2, None)
        .await?;
    let fetched2 = persist
        .knowledge()
        .fetch_session_knowledge(session1_uuid)
        .await?
        .context("row must still exist after second upsert")?;
    assert_eq!(fetched2.topics, topics2);
    assert!(fetched2.learnings.is_empty());
    assert!(fetched2.related_session_ids.is_empty());
    assert_eq!(fetched2.verdict_id, None);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory.session_knowledge WHERE session_id = $1")
            .bind(session1_uuid)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 1, "upsert must replace, never duplicate, the row");

    let many = persist
        .knowledge()
        .fetch_session_knowledge_many(&[session1_uuid, session2_uuid])
        .await?;
    assert_eq!(many.len(), 1);
    assert_eq!(many[0].session_id, session1_uuid);
    assert_eq!(many[0].topics, topics2);

    let none = persist
        .knowledge()
        .fetch_session_knowledge_many(&[session2_uuid])
        .await?;
    assert!(none.is_empty());

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
