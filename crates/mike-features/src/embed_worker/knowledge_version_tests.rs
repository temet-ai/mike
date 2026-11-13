use super::knowledge::{model_keys, sweep};
use super::knowledge_key::{keys_for, previous_key_for};
use super::knowledge_tests::{breaker, scan, source, vector};
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ModelKeys, ProjectIdentity};
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

const NOTE_TITLE: &str = "Pick the vector form";

async fn embedder(model: &str) -> (MockServer, EmbeddingService) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(|req: &Request| {
            let count = serde_json::from_slice::<Value>(&req.body)
                .ok()
                .and_then(|body| body["input"].as_array().map(Vec::len))
                .unwrap_or(0);
            let data: Vec<Value> = (0..count).map(|_| json!({"embedding": vector()})).collect();
            ResponseTemplate::new(200).set_body_json(json!({"data": data}))
        })
        .mount(&server)
        .await;
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: model.into(),
        chunk_max_chars: 80,
    });
    (server, service)
}

async fn posted_inputs(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|request| serde_json::from_slice::<Value>(&request.body).ok())
        .flat_map(|body| body["input"].as_array().cloned().unwrap_or_default())
        .filter_map(|input| input.as_str().map(str::to_owned))
        .collect()
}

struct Seeded {
    db: crate::store::persistence::test_support::TestDbHandle,
    handle: PersistHandle,
    project: ProjectIdentity,
}

async fn seeded() -> anyhow::Result<Seeded> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("Versions", Uuid::new_v4())
        .await?;
    Ok(Seeded {
        db,
        handle,
        project,
    })
}

async fn with_note(seeded: &Seeded) -> anyhow::Result<()> {
    let session: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, title)
         VALUES ($1, $2, now(), now(), 'Version session') RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(seeded.project.id)
    .fetch_one(seeded.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body)
         VALUES ($1, 'decision', $2, repeat('We pick the titled form for every source. ', 4))",
    )
    .bind(session)
    .bind(NOTE_TITLE)
    .execute(seeded.handle.pool())
    .await?;
    Ok(())
}

async fn with_artifact(seeded: &Seeded) -> anyhow::Result<()> {
    let location = source(&seeded.handle, &seeded.project.project_uuid.to_string()).await?;
    scan(&seeded.handle, location, 0, &"Durable evidence. ".repeat(8)).await
}

async fn move_to_key(seeded: &Seeded, key: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE memory.knowledge_embedding_state SET model_key = $1")
        .bind(key)
        .execute(seeded.handle.pool())
        .await?;
    Ok(())
}

async fn found_by_meaning(seeded: &Seeded, keys: ModelKeys, model: &str) -> anyhow::Result<usize> {
    let embedding = (
        crate::store::operator_turns::vector_to_text_literal(&vector()),
        keys,
        model.to_owned(),
    );
    let query = KnowledgeQuery {
        project: seeded.project.project_uuid.to_string(),
        query: "unrelated wording".into(),
        ..Default::default()
    };
    let found = seeded
        .handle
        .knowledge()
        .search_project_knowledge_hybrid(&query, Some(&embedding))
        .await?;
    Ok(found["items"].as_array().map_or(0, Vec::len))
}

#[tokio::test]
async fn sweep_with_embeddinggemma_posts_the_header_in_the_title_slot_of_every_chunk()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    with_note(&seeded).await?;
    let (server, service) = embedder("embeddinggemma").await;

    sweep(&seeded.handle, &service, &mut breaker()).await?;

    let inputs = posted_inputs(&server).await;
    let prefix = format!("title: note, decision: {NOTE_TITLE} | text: ");
    assert!(inputs.len() > 1, "{inputs:?}");
    assert!(
        inputs.iter().all(|input| input.starts_with(&prefix)),
        "{inputs:?}"
    );
    let stored: Vec<String> =
        sqlx::query_scalar("SELECT body FROM memory.knowledge_embedding_chunks")
            .fetch_all(seeded.handle.pool())
            .await?;
    assert!(
        stored.iter().all(|body| !body.contains("title: note")),
        "{stored:?}"
    );
    seeded.db.cleanup().await
}

#[tokio::test]
async fn sweep_with_a_model_that_has_no_title_slot_puts_the_header_on_its_own_first_line()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    with_note(&seeded).await?;
    let (nomic_server, nomic) = embedder("nomic-embed-text-v1.5").await;
    let (plain_server, plain) = embedder("all-minilm").await;

    sweep(&seeded.handle, &nomic, &mut breaker()).await?;
    sqlx::query("DELETE FROM memory.knowledge_embedding_state")
        .execute(seeded.handle.pool())
        .await?;
    sweep(&seeded.handle, &plain, &mut breaker()).await?;

    let first_line = format!("note, decision: {NOTE_TITLE}\n");
    let nomic_inputs = posted_inputs(&nomic_server).await;
    let plain_inputs = posted_inputs(&plain_server).await;
    assert!(!nomic_inputs.is_empty() && !plain_inputs.is_empty());
    assert!(
        nomic_inputs
            .iter()
            .all(|i| i.starts_with(&format!("search_document: {first_line}")))
    );
    assert!(plain_inputs.iter().all(|i| i.starts_with(&first_line)));
    seeded.db.cleanup().await
}

#[tokio::test]
async fn search_by_meaning_still_returns_a_source_embedded_under_the_previous_key()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    with_artifact(&seeded).await?;
    let (_server, service) = embedder("embeddinggemma").await;
    sweep(&seeded.handle, &service, &mut breaker()).await?;
    let keys = model_keys(&service);
    let previous = previous_key_for("embeddinggemma", 80)
        .ok_or_else(|| anyhow::anyhow!("embeddinggemma has no previous key"))?;
    move_to_key(&seeded, &previous).await?;

    assert_eq!(
        found_by_meaning(&seeded, keys.clone(), "embeddinggemma").await?,
        1
    );
    let current_only = ModelKeys::only(keys.current());
    assert_eq!(
        found_by_meaning(&seeded, current_only, "embeddinggemma").await?,
        0
    );
    let health = seeded
        .handle
        .knowledge()
        .knowledge_embedding_health(Some(seeded.project.id), &keys)
        .await?;
    assert_eq!(
        (health["ready"].as_i64(), health["current_version"].as_i64()),
        (Some(1), Some(0))
    );
    let counts = seeded
        .handle
        .knowledge()
        .knowledge_embedding_state_counts(&keys)
        .await?;
    assert_eq!((counts.ready, counts.current_version), (1, 0));
    seeded.db.cleanup().await
}

#[tokio::test]
async fn sweep_embeds_a_previous_key_source_again_and_its_key_becomes_current() -> anyhow::Result<()>
{
    let seeded = seeded().await?;
    with_artifact(&seeded).await?;
    let (server, service) = embedder("embeddinggemma").await;
    sweep(&seeded.handle, &service, &mut breaker()).await?;
    let keys = model_keys(&service);
    let previous = previous_key_for("embeddinggemma", 80)
        .ok_or_else(|| anyhow::anyhow!("embeddinggemma has no previous key"))?;
    move_to_key(&seeded, &previous).await?;
    let before = posted_inputs(&server).await.len();

    assert_eq!(
        seeded
            .handle
            .knowledge()
            .pending_knowledge_embeddings(keys.current())
            .await?
            .len(),
        1
    );
    let report = sweep(&seeded.handle, &service, &mut breaker()).await?;

    assert_eq!(report.map(|r| r.embedded), Some(1));
    assert!(posted_inputs(&server).await.len() > before);
    let stored: Vec<String> =
        sqlx::query_scalar("SELECT model_key FROM memory.knowledge_embedding_state")
            .fetch_all(seeded.handle.pool())
            .await?;
    assert_eq!(stored, [keys.current().to_owned()]);
    let health = seeded
        .handle
        .knowledge()
        .knowledge_embedding_health(Some(seeded.project.id), &keys)
        .await?;
    assert_eq!(
        (health["ready"].as_i64(), health["current_version"].as_i64()),
        (Some(1), Some(1))
    );
    assert!(
        seeded
            .handle
            .knowledge()
            .pending_knowledge_embeddings(keys.current())
            .await?
            .is_empty()
    );
    seeded.db.cleanup().await
}

#[test]
fn only_embeddinggemma_has_a_previous_key_and_it_differs_from_the_current_one() {
    let gemma = keys_for("embeddinggemma", 1500);
    assert_eq!(gemma.accepted().len(), 2);
    assert_ne!(gemma.accepted()[0], gemma.accepted()[1]);
    assert_eq!(keys_for("nomic-embed-text-v1.5", 1500).accepted().len(), 1);
    assert_eq!(keys_for("all-minilm", 1500).accepted().len(), 1);
}
