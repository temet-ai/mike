use super::knowledge::{model_key, sweep};
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::circuit_breaker::CircuitBreaker;
use crate::store::persistence::{
    PersistHandle,
    project_knowledge::{KnowledgeQuery, LocationInput, SourceInput},
    test_support::acquire_ephemeral_db,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

pub(super) async fn source(handle: &PersistHandle, project: &str) -> anyhow::Result<Uuid> {
    let location = handle
        .register_project_location(&LocationInput {
            project: project.into(),
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
    Ok(serde_json::from_value(source["source_uuid"].clone())?)
}

pub(super) async fn scan(
    handle: &PersistHandle,
    source: Uuid,
    generation: i64,
    body: &str,
) -> anyhow::Result<()> {
    handle.ingest_knowledge_scan(source, &serde_json::from_value(json!({"generation":generation,"documents":[{
        "relative_path":"design.md", "title":"Design", "kind":"design", "status":"accepted", "metadata":{},
        "body":body, "content_hash":Sha256::digest(body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>()
    }]}))?).await?;
    Ok(())
}

pub(super) fn vector() -> Vec<f32> {
    let mut v = vec![0.0; 768];
    v[0] = 1.0;
    v
}
pub(super) fn breaker() -> CircuitBreaker {
    CircuitBreaker::new(3, Duration::from_secs(1), Duration::from_secs(10))
}

#[tokio::test]
async fn project_knowledge_semantic_worker_scope_revision_and_retry() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .define_knowledge_project("Semantic", Uuid::new_v4())
        .await?;
    let other = handle
        .define_knowledge_project("Other", Uuid::new_v4())
        .await?;
    let source = source(&handle, &project.project_uuid.to_string()).await?;
    scan(
        &handle,
        source,
        0,
        &"Durable architecture evidence. ".repeat(12),
    )
    .await?;
    let (server, service) = embedding_server().await;
    sweep(&handle, &service, &mut breaker()).await?;
    let key = model_key(&service);
    let health = handle
        .knowledge_embedding_health(Some(project.id), &key)
        .await?;
    assert_eq!(health["ready"], 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.knowledge_embedding_chunks")
        .fetch_one(handle.pool())
        .await?;
    assert!(count > 1);
    let query = KnowledgeQuery {
        project: project.project_uuid.to_string(),
        query: "unrelated wording".into(),
        ..Default::default()
    };
    assert!(
        handle.search_project_knowledge(&query).await?["items"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    let embedding = (
        crate::store::operator_turns::vector_to_text_literal(&vector()),
        key.clone(),
        service.model_name(),
    );
    let result = handle
        .search_project_knowledge_hybrid(&query, Some(&embedding))
        .await?;
    assert_eq!(result["items"].as_array().map(Vec::len), Some(1));
    assert!(result["items"][0]["semantic_rank"].as_f64().unwrap_or(0.0) > 0.9);
    let historical = result["items"][0]["reference"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing reference"))?;
    let other_query = KnowledgeQuery {
        project: other.project_uuid.to_string(),
        ..query
    };
    assert!(
        handle
            .search_project_knowledge_hybrid(&other_query, Some(&embedding))
            .await?["items"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    scan(&handle, source, 1, "Replacement version").await?;
    assert_eq!(
        handle
            .read_project_knowledge(Some(&project.project_uuid.to_string()), historical)
            .await?["current"],
        false
    );
    let query = KnowledgeQuery {
        project: project.project_uuid.to_string(),
        ..other_query
    };
    assert!(
        handle
            .search_project_knowledge_hybrid(&query, Some(&embedding))
            .await?["items"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    verify_retry(&handle, &server, &service, &key, project.id).await?;
    db.cleanup().await?;
    Ok(())
}

pub(super) async fn embedding_server() -> (MockServer, EmbeddingService) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(|req: &Request| {
            let Ok(body) = serde_json::from_slice::<Value>(&req.body) else {
                return ResponseTemplate::new(400);
            };
            let Some(input) = body["input"].as_array() else {
                return ResponseTemplate::new(400);
            };
            let data: Vec<Value> = input
                .iter()
                .map(|_| json!({"embedding":vector()}))
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({"data":data}))
        })
        .mount(&server)
        .await;
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: "test-model".into(),
        chunk_max_chars: 80,
    });
    (server, service)
}

async fn verify_retry(
    handle: &PersistHandle,
    server: &MockServer,
    service: &EmbeddingService,
    key: &str,
    project: i64,
) -> anyhow::Result<()> {
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(server)
        .await;
    sweep(handle, service, &mut breaker()).await?;
    assert!(handle.pending_knowledge_embeddings(key).await?.is_empty());
    assert_eq!(
        handle
            .knowledge_embedding_health(Some(project), key)
            .await?["failed"],
        1
    );
    let changed = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: "new-model".into(),
        chunk_max_chars: 80,
    });
    assert_eq!(
        handle
            .pending_knowledge_embeddings(&model_key(&changed))
            .await?
            .len(),
        1
    );
    Ok(())
}
