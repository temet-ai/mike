use std::sync::Arc;

use serde_json::json;
use tokio::sync::Mutex;

use super::running_services::{apply_stored_rows, sync_running_services};
use crate::analysis::{EmbeddingConfig, EmbeddingService, RerankConfig, RerankService};
use crate::config::{AppState, ProxyConfig, SettingsWriteGuard};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn machine(database_url: &str) -> anyhow::Result<AppState> {
    let handle = PersistHandle::connect(database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(handle.clone());
    state.settings_service = Some(
        crate::settings::service::SettingsService::reconcile_on_startup(
            state.settings_overlay.clone(),
            Arc::new(Mutex::new(SettingsWriteGuard)),
            handle,
        )
        .await?,
    );
    Ok(state)
}

async fn sync(state: &AppState) -> anyhow::Result<Vec<String>> {
    let persist = state
        .persist
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("machine has no database handle"))?;
    Ok(sync_running_services(persist, &state.embedding_service(), &state.rerank_service()).await?)
}

#[tokio::test]
async fn sync_running_services_with_embedding_model_written_by_another_machine_updates_the_running_service()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let machine_a = machine(&db.database_url).await?;
    let machine_b = machine(&db.database_url).await?;
    let before = machine_b.embedding_service().model_name();
    let service_a = machine_a
        .settings_service()
        .ok_or_else(|| anyhow::anyhow!("machine A has no settings service"))?;

    service_a
        .set("embedding.model", json!("nomic-embed-text-v1.5"))
        .await?;
    let changed = sync(&machine_b).await?;

    assert_ne!(before, "nomic-embed-text-v1.5");
    assert_eq!(
        machine_b.embedding_service().model_name(),
        "nomic-embed-text-v1.5"
    );
    assert_eq!(changed, vec!["embedding.model".to_owned()]);
    assert_eq!(
        sync(&machine_b).await?,
        Vec::<String>::new(),
        "a second pass over unchanged rows must change nothing"
    );
    db.cleanup().await
}

#[tokio::test]
async fn sync_running_services_with_rerank_settings_written_by_another_machine_updates_the_running_service()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let machine_a = machine(&db.database_url).await?;
    let machine_b = machine(&db.database_url).await?;
    let service_a = machine_a
        .settings_service()
        .ok_or_else(|| anyhow::anyhow!("machine A has no settings service"))?;

    service_a
        .set("rerank.model", json!("test-reranker-model"))
        .await?;
    service_a.set("rerank.candidates", json!(7)).await?;
    sync(&machine_b).await?;

    let config = machine_b.rerank_service().current_config();
    assert_eq!(config.model, "test-reranker-model");
    assert_eq!(config.candidates, 7);
    db.cleanup().await
}

#[test]
fn apply_stored_rows_with_values_equal_to_the_running_config_reports_no_change() {
    let embedding = EmbeddingService::new(&EmbeddingConfig {
        model: "same-model".into(),
        ..EmbeddingConfig::default()
    });
    let rerank = RerankService::new(&RerankConfig::default());
    let rows = vec![
        ("embedding.model".to_owned(), json!("same-model")),
        ("llm.max_output_tokens".to_owned(), json!(1)),
    ];

    assert!(apply_stored_rows(&rows, &embedding, &rerank).is_empty());
}

#[test]
fn apply_stored_rows_with_a_different_embedding_model_reports_the_key_and_applies_it() {
    let embedding = EmbeddingService::new(&EmbeddingConfig {
        model: "old-model".into(),
        ..EmbeddingConfig::default()
    });
    let rerank = RerankService::new(&RerankConfig::default());
    let rows = vec![("embedding.model".to_owned(), json!("new-model"))];

    let changed = apply_stored_rows(&rows, &embedding, &rerank);

    assert_eq!(changed, vec!["embedding.model".to_owned()]);
    assert_eq!(embedding.model_name(), "new-model");
}

#[test]
fn apply_stored_rows_with_a_value_of_the_wrong_type_leaves_the_service_unchanged() {
    let embedding = EmbeddingService::new(&EmbeddingConfig {
        model: "old-model".into(),
        ..EmbeddingConfig::default()
    });
    let rerank = RerankService::new(&RerankConfig::default());
    let rows = vec![("embedding.model".to_owned(), json!(5))];

    assert!(apply_stored_rows(&rows, &embedding, &rerank).is_empty());
    assert_eq!(embedding.model_name(), "old-model");
}
