use super::knowledge::{model_key, sweep};
use super::knowledge_tests::{breaker, embedding_server, vector};
use crate::store::persistence::{
    PersistHandle,
    project_knowledge::{KnowledgeQuery, ProjectIdentity},
    test_support::acquire_ephemeral_db,
};
use uuid::Uuid;

const OVERVIEW: &str =
    "The session wired the cormorant adapter to the second key and removed the old retry loop.";

async fn session_with_overview(
    handle: &PersistHandle,
    project: &ProjectIdentity,
) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, title,
                                        overview, overview_generated_at)
         VALUES ($1, $2, now(), now(), 'Cormorant adapter work', $3, now())",
    )
    .bind(session)
    .bind(project.id)
    .bind(OVERVIEW)
    .execute(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO memory.session_knowledge (session_id, topics, learnings, generated_at)
         VALUES ($1, ARRAY['adapters', 'keys'], ARRAY['Use the second key'], now())",
    )
    .bind(session)
    .execute(handle.pool())
    .await?;
    Ok(session)
}

#[tokio::test]
async fn sweep_embeds_a_session_overview_with_its_topics_and_search_finds_it_by_meaning()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .define_knowledge_project("Overviews", Uuid::new_v4())
        .await?;
    let session = session_with_overview(&handle, &project).await?;
    let (_server, service) = embedding_server().await;
    let key = model_key(&service);
    let before = handle
        .knowledge_embedding_health(Some(project.id), &key)
        .await?;
    assert_eq!(
        (before["total"].as_i64(), before["ready"].as_i64()),
        (Some(2), Some(0))
    );

    sweep(&handle, &service, &mut breaker()).await?;

    let after = handle
        .knowledge_embedding_health(Some(project.id), &key)
        .await?;
    assert_eq!(
        (after["total"].as_i64(), after["ready"].as_i64()),
        (Some(2), Some(2))
    );
    let bodies: Vec<String> = sqlx::query_scalar(
        "SELECT c.body FROM memory.knowledge_embedding_state e
           JOIN memory.knowledge_embedding_chunks c ON c.state_id = e.id
          WHERE e.source_kind = 'session' AND e.model_key = $1 AND e.completed_at IS NOT NULL
          ORDER BY c.ordinal",
    )
    .bind(&key)
    .fetch_all(handle.pool())
    .await?;
    assert!(bodies.len() > 1, "{bodies:?}");
    assert!(
        bodies.concat().contains("Topics: adapters, keys"),
        "{bodies:?}"
    );
    let embedding = (
        crate::store::operator_turns::vector_to_text_literal(&vector()),
        key,
        service.model_name(),
    );
    let query = KnowledgeQuery {
        project: project.project_uuid.to_string(),
        query: "unrelated wording".into(),
        kind: Some("session".into()),
        ..Default::default()
    };
    let found = handle
        .search_project_knowledge_hybrid(&query, Some(&embedding))
        .await?;
    assert_eq!(found["items"].as_array().map(Vec::len), Some(1), "{found}");
    assert_eq!(found["items"][0]["reference"], format!("session:{session}"));
    assert!(found["items"][0]["semantic_rank"].as_f64().unwrap_or(0.0) > 0.9);
    db.cleanup().await
}
