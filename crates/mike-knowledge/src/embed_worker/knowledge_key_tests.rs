use super::knowledge::{model_key, model_keys, sweep_if_leader};
use super::knowledge_key::{LEASE_ROLE, rekey_url_keyed_rows, unversioned_v2_key};
use super::knowledge_tests::{breaker, embedding_server, scan, source, vector};
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::{
    PersistHandle, semantic::NewEmbedder, test_support::acquire_ephemeral_db,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const LOOPBACK: &str = "http://127.0.0.1:11434/v1";
const LAN: &str = "http://192.168.1.20:11434/v1";
const OTHER_HOST: &str = "http://10.0.0.9:11434/v1";
const UNRECORDED: &str = "http://unrecorded.invalid:1/v1";
const MODEL: &str = "nomic-embed-text-v1.5";

fn legacy_key(url: &str) -> String {
    Sha256::digest(format!("knowledge-v1:{url}:{MODEL}:1024"))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn loopback_service() -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: LOOPBACK.into(),
        model: MODEL.into(),
        chunk_max_chars: 1024,
    })
}

async fn fresh_db() -> anyhow::Result<(
    crate::store::persistence::test_support::TestDbHandle,
    PersistHandle,
)> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok((db, handle))
}

async fn seed_state(
    handle: &PersistHandle,
    first_id: i64,
    key: &str,
    rows: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.knowledge_embedding_state(source_kind, source_id, model_key, completed_at)
         SELECT 'artifact', $1 + g, $2, now() - interval '1 day' FROM generate_series(1, $3) g",
    )
    .bind(first_id)
    .bind(key)
    .bind(rows)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn keyed_count(handle: &PersistHandle, key: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_embedding_state WHERE model_key = $1",
    )
    .bind(key)
    .fetch_one(handle.pool())
    .await?)
}

async fn record_urls(handle: &PersistHandle) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO config.settings(key, value) VALUES ('embedding.url', to_jsonb($1::text))",
    )
    .bind(LAN)
    .execute(handle.pool())
    .await?;
    handle
        .knowledge()
        .insert_semantic_embedder(&NewEmbedder {
            embedder_key: "test-embedder",
            model: MODEL,
            dim: 768,
            chunk_max_chars: 1024,
            prefix_version: "doc-prefix-v1",
            url: OTHER_HOST,
            canary_version: "test",
            canary_vectors: json!({}),
            first_seen_at: chrono::Utc::now(),
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn rekey_moves_every_url_keyed_row_to_the_host_independent_key() -> anyhow::Result<()> {
    let (db, handle) = fresh_db().await?;
    let service = loopback_service();
    let target = model_key(&service);
    record_urls(&handle).await?;
    seed_state(&handle, 1000, &legacy_key(LOOPBACK), 40).await?;
    seed_state(&handle, 2000, &legacy_key(LAN), 7).await?;
    seed_state(&handle, 3000, &legacy_key(OTHER_HOST), 3).await?;
    seed_state(&handle, 4000, &legacy_key(UNRECORDED), 2).await?;
    seed_state(&handle, 5000, &target, 5).await?;
    sqlx::query(
        "INSERT INTO memory.knowledge_embedding_chunks(state_id, ordinal, body, embedding)
         SELECT id, 0, 'kept', $1::public.vector FROM memory.knowledge_embedding_state
          WHERE source_kind = 'artifact' AND source_id = 2001",
    )
    .bind(crate::store::operator_turns::vector_to_text_literal(
        &vector(),
    ))
    .execute(handle.pool())
    .await?;

    assert_eq!(rekey_url_keyed_rows(&handle, &service).await?, 50);
    assert_eq!(keyed_count(&handle, &target).await?, 55);
    assert_eq!(keyed_count(&handle, &legacy_key(UNRECORDED)).await?, 2);
    assert_eq!(rekey_url_keyed_rows(&handle, &service).await?, 0);
    let kept: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_embedding_chunks c
         JOIN memory.knowledge_embedding_state e ON e.id = c.state_id
         WHERE e.model_key = $1 AND e.completed_at < now() - interval '12 hours'",
    )
    .bind(&target)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(kept, 1);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rekey_without_matching_keys_changes_nothing() -> anyhow::Result<()> {
    let (db, handle) = fresh_db().await?;
    seed_state(&handle, 26000, &legacy_key(UNRECORDED), 4).await?;
    assert_eq!(rekey_url_keyed_rows(&handle, &loopback_service()).await?, 0);
    assert_eq!(keyed_count(&handle, &legacy_key(UNRECORDED)).await?, 4);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rekey_with_unversioned_v2_rows_leaves_them_for_the_worker_to_redo() -> anyhow::Result<()> {
    let (db, handle) = fresh_db().await?;
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: LOOPBACK.into(),
        model: "embeddinggemma".into(),
        chunk_max_chars: 1024,
    });
    let old_key = unversioned_v2_key("embeddinggemma", 1024);
    assert_ne!(old_key, model_key(&service));
    seed_state(&handle, 31000, &old_key, 6).await?;
    assert_eq!(rekey_url_keyed_rows(&handle, &service).await?, 0);
    assert_eq!(keyed_count(&handle, &old_key).await?, 6);
    assert_eq!(keyed_count(&handle, &model_key(&service)).await?, 0);
    db.cleanup().await?;
    Ok(())
}

fn disabled(service: &EmbeddingService) -> EmbeddingService {
    let mut config = service.current_config();
    config.enabled = false;
    EmbeddingService::new(&config)
}

async fn machine_handle(url: &str) -> anyhow::Result<PersistHandle> {
    Ok(PersistHandle::connect(url)
        .await?
        .with_llm_machine(Uuid::new_v4()))
}

#[tokio::test]
async fn sweep_runs_only_on_the_machine_holding_the_lease() -> anyhow::Result<()> {
    let (db, seed) = fresh_db().await?;
    let project = seed
        .knowledge()
        .define_knowledge_project("Leased", Uuid::new_v4())
        .await?;
    let location = source(&seed, &project.project_uuid.to_string()).await?;
    scan(&seed, location, 0, &"Leased sweep evidence. ".repeat(12)).await?;
    let (_server, service) = embedding_server().await;
    let key = model_keys(&service);
    let first = machine_handle(&db.database_url).await?;
    let second = machine_handle(&db.database_url).await?;

    assert!(!sweep_if_leader(&second, &disabled(&service), &mut breaker()).await?);
    assert!(sweep_if_leader(&first, &service, &mut breaker()).await?);
    let health = seed
        .knowledge()
        .knowledge_embedding_health(None, &key)
        .await?;
    assert_eq!(health["ready"], 1);

    scan(&seed, location, 1, "Second revision needing a vector").await?;
    assert!(!sweep_if_leader(&second, &service, &mut breaker()).await?);
    let health = seed
        .knowledge()
        .knowledge_embedding_health(None, &key)
        .await?;
    assert_eq!(health["pending"], 1);

    sqlx::query(
        "UPDATE llm.worker_leases SET heartbeat_at = now() - interval '1 hour' WHERE role = $1",
    )
    .bind(LEASE_ROLE)
    .execute(seed.pool())
    .await?;
    let takeover = machine_handle(&db.database_url).await?;
    assert!(sweep_if_leader(&takeover, &service, &mut breaker()).await?);
    let health = seed
        .knowledge()
        .knowledge_embedding_health(None, &key)
        .await?;
    assert_eq!(health["pending"], 0);
    db.cleanup().await?;
    Ok(())
}
