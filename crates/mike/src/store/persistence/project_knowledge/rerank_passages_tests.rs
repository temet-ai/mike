use uuid::Uuid;

use super::tests::document;
use super::{KnowledgeQuery, LocationInput, ScanInput, SourceInput};
use crate::store::operator_turns::vector_to_text_literal;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

const MODEL: &str = "test-model";

fn axis(index: usize) -> Vec<f32> {
    let mut v = vec![0.0; 768];
    v[index] = 1.0;
    v
}

struct Seeded {
    db: TestDbHandle,
    handle: PersistHandle,
    reference: String,
}

async fn seeded() -> anyhow::Result<Seeded> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .define_knowledge_project("Passages", Uuid::new_v4())
        .await?;
    let location = handle
        .register_project_location(&LocationInput {
            project: project.project_uuid.to_string(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "test".into(),
            root_path: "/work".into(),
        })
        .await?;
    let source = handle
        .register_knowledge_source(&SourceInput {
            location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
            name: "artifacts".into(),
            root_path: "/work/artifacts".into(),
        })
        .await?;
    handle
        .ingest_knowledge_scan(
            serde_json::from_value(source["source_uuid"].clone())?,
            &ScanInput {
                generation: 0,
                documents: vec![document("Cache warmth decisions", "accepted")],
            },
        )
        .await?;
    let found = handle
        .search_project_knowledge(&KnowledgeQuery {
            project: project.project_uuid.to_string(),
            query: "cache".into(),
            ..Default::default()
        })
        .await?;
    let reference = found["items"][0]["reference"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no artifact found: {found}"))?
        .to_owned();
    Ok(Seeded {
        db,
        handle,
        reference,
    })
}

async fn embed_chunks(
    seeded: &Seeded,
    model: &str,
    chunks: &[(&str, usize)],
) -> anyhow::Result<()> {
    let revision: i64 = seeded.reference.trim_start_matches("artifact:").parse()?;
    let state: i64 = sqlx::query_scalar(
        "INSERT INTO memory.knowledge_embedding_state (source_kind, source_id, model_key, completed_at)
         VALUES ('artifact', $1, $2, now()) RETURNING id",
    )
    .bind(revision)
    .bind(model)
    .fetch_one(seeded.handle.pool())
    .await?;
    for (ordinal, (body, axis_index)) in chunks.iter().enumerate() {
        sqlx::query(
            "INSERT INTO memory.knowledge_embedding_chunks (state_id, ordinal, body, embedding)
             VALUES ($1, $2, $3, $4::text::public.vector)",
        )
        .bind(state)
        .bind(i32::try_from(ordinal)?)
        .bind(*body)
        .bind(vector_to_text_literal(&axis(*axis_index)))
        .execute(seeded.handle.pool())
        .await?;
    }
    Ok(())
}

fn query_vector(axis_index: usize, model: &str) -> (String, String, String) {
    (
        vector_to_text_literal(&axis(axis_index)),
        model.to_owned(),
        "embedder".to_owned(),
    )
}

async fn close(seeded: Seeded) -> anyhow::Result<()> {
    seeded.handle.pool().close().await;
    seeded.db.cleanup().await
}

#[tokio::test]
async fn rerank_passages_with_a_query_vector_returns_the_nearest_chunk() -> anyhow::Result<()> {
    let seeded = seeded().await?;
    embed_chunks(
        &seeded,
        MODEL,
        &[("intro chunk", 0), ("the cache chunk", 5)],
    )
    .await?;

    let passages = seeded
        .handle
        .knowledge_rerank_passages(
            std::slice::from_ref(&seeded.reference),
            Some(&query_vector(5, MODEL)),
        )
        .await?;

    assert_eq!(
        passages.get(&seeded.reference).map(String::as_str),
        Some("the cache chunk")
    );
    close(seeded).await
}

#[tokio::test]
async fn rerank_passages_without_a_query_vector_returns_the_first_chunk() -> anyhow::Result<()> {
    let seeded = seeded().await?;
    embed_chunks(
        &seeded,
        MODEL,
        &[("intro chunk", 0), ("the cache chunk", 5)],
    )
    .await?;

    let passages = seeded
        .handle
        .knowledge_rerank_passages(std::slice::from_ref(&seeded.reference), None)
        .await?;

    assert_eq!(
        passages.get(&seeded.reference).map(String::as_str),
        Some("intro chunk")
    );
    close(seeded).await
}

#[tokio::test]
async fn rerank_passages_for_another_embedding_model_returns_nothing() -> anyhow::Result<()> {
    let seeded = seeded().await?;
    embed_chunks(&seeded, "old-model", &[("stale chunk", 0)]).await?;

    let passages = seeded
        .handle
        .knowledge_rerank_passages(
            std::slice::from_ref(&seeded.reference),
            Some(&query_vector(0, MODEL)),
        )
        .await?;

    assert!(passages.is_empty());
    close(seeded).await
}

#[tokio::test]
async fn rerank_passages_with_unparseable_or_unembedded_references_returns_nothing()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    let references: Vec<String> = [
        "artifact:not-a-number",
        "lesson:not-a-uuid",
        "note:",
        "turn:77",
        "work_thread:slug",
        "no-colon",
    ]
    .iter()
    .map(|r| (*r).to_owned())
    .collect();

    let passages = seeded
        .handle
        .knowledge_rerank_passages(&references, Some(&query_vector(0, MODEL)))
        .await?;

    assert!(passages.is_empty());
    close(seeded).await
}

async fn turn_with_chunks(
    seeded: &Seeded,
    model: &str,
    chunks: &[(&str, &str, usize)],
) -> anyhow::Result<String> {
    let pool = seeded.handle.pool();
    let project_id =
        crate::store::persistence::test_support::seed_project(pool, "w-passage-turns").await?;
    let session = "00000000-0000-4000-8000-0000000000e1";
    crate::store::persistence::test_support::seed_two_sessions(
        pool,
        session,
        "00000000-0000-4000-8000-0000000000e2",
        project_id,
    )
    .await?;
    crate::store::persistence::test_support::seed_session_turns(pool, session, project_id, 1)
        .await?;
    let turn: i64 =
        sqlx::query_scalar("SELECT id FROM memory.operator_turns WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    for (index, (role, text, axis_index)) in chunks.iter().enumerate() {
        sqlx::query(
            "INSERT INTO memory.operator_turn_chunks
                (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
             SELECT $1, $2, cr.id, $3, $4::text::vector, $5
               FROM memory.chunk_roles cr WHERE cr.code = $6",
        )
        .bind(turn)
        .bind(i32::try_from(index)?)
        .bind(*text)
        .bind(vector_to_text_literal(&axis(*axis_index)))
        .bind(model)
        .bind(*role)
        .execute(pool)
        .await?;
    }
    Ok(format!("turn:{turn}"))
}

#[tokio::test]
async fn rerank_passages_for_a_turn_returns_its_chunk_nearest_the_query() -> anyhow::Result<()> {
    let seeded = seeded().await?;
    let turn = turn_with_chunks(
        &seeded,
        "embedder",
        &[
            ("prompt", "get the changes ready", 0),
            ("assistant", "branch feat/ABC-1234-upgrade is ready", 5),
        ],
    )
    .await?;

    let near = seeded
        .handle
        .knowledge_rerank_passages(std::slice::from_ref(&turn), Some(&query_vector(5, MODEL)))
        .await?;
    let first = seeded
        .handle
        .knowledge_rerank_passages(std::slice::from_ref(&turn), None)
        .await?;

    assert_eq!(
        near.get(&turn).map(String::as_str),
        Some("branch feat/ABC-1234-upgrade is ready")
    );
    assert_eq!(
        first.get(&turn).map(String::as_str),
        Some("get the changes ready")
    );
    close(seeded).await
}
