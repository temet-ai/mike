use super::knowledge::{model_keys, sweep};
use super::knowledge_tests::{breaker, scan, source};
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::{
    PersistHandle,
    project_knowledge::{KNOWLEDGE_COUNT_CAP, ModelKeys},
    test_support::acquire_ephemeral_db,
};
use serde_json::json;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn service_at(server: &MockServer) -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: format!("counts-{}", Uuid::new_v4()),
        chunk_max_chars: 80,
    })
}

async fn answering(status: u16) -> MockServer {
    let server = MockServer::start().await;
    let mut vector = vec![0.0_f32; 768];
    vector[0] = 1.0;
    let body = json!({"data": [{"embedding": vector}]});
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn knowledge_state_counts_track_ready_and_current_failures() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("Counts", Uuid::new_v4())
        .await?;
    let location = source(&handle, &project.project_uuid.to_string()).await?;
    scan(&handle, location, 0, "Counted evidence").await?;
    let healthy = answering(200).await;
    let service = service_at(&healthy);
    let key = model_keys(&service);

    let report = sweep(&handle, &service, &mut breaker())
        .await?
        .ok_or_else(|| anyhow::anyhow!("sweep skipped"))?;
    assert_eq!((report.embedded, report.failed, report.pending), (1, 0, 0));
    assert!(!report.has_pending());
    let counts = handle
        .knowledge()
        .knowledge_embedding_state_counts(&key)
        .await?;
    assert_eq!((counts.ready, counts.failed), (1, 0));

    let failing = answering(503).await;
    let broken = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: failing.uri(),
        model: service.model_name(),
        chunk_max_chars: 80,
    });
    scan(&handle, location, 1, "Revised evidence").await?;
    let report = sweep(&handle, &broken, &mut breaker())
        .await?
        .ok_or_else(|| anyhow::anyhow!("sweep skipped"))?;
    assert_eq!(report.failed, 1);
    let counts = handle
        .knowledge()
        .knowledge_embedding_state_counts(&key)
        .await?;
    assert_eq!(counts.failed, 1);

    sqlx::query("UPDATE memory.knowledge_documents SET deleted_at = now() WHERE source_uuid = $1")
        .bind(location)
        .execute(handle.pool())
        .await?;
    let counts = handle
        .knowledge()
        .knowledge_embedding_state_counts(&key)
        .await?;
    assert_eq!(
        counts.failed, 0,
        "a failure on a removed source is not current"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn knowledge_state_counts_stop_one_past_the_cap() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let key = format!("cap-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO memory.knowledge_embedding_state
             (source_kind, source_id, model_key, completed_at, last_error)
         SELECT 'artifact', g, $1, now(), '[permanent] orphaned'
           FROM generate_series(1, $2::bigint + 5) g",
    )
    .bind(&key)
    .bind(KNOWLEDGE_COUNT_CAP)
    .execute(handle.pool())
    .await?;

    let counts = handle
        .knowledge()
        .knowledge_embedding_state_counts(&ModelKeys::only(key))
        .await?;

    assert_eq!(counts.ready, KNOWLEDGE_COUNT_CAP + 1);
    assert_eq!(
        counts.failed, 0,
        "failures without a current source are not counted"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn knowledge_state_counts_unknown_model_is_zero() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let counts = handle
        .knowledge()
        .knowledge_embedding_state_counts(&ModelKeys::only(format!("none-{}", Uuid::new_v4())))
        .await?;
    assert_eq!((counts.ready, counts.failed), (0, 0));
    db.cleanup().await?;
    Ok(())
}
