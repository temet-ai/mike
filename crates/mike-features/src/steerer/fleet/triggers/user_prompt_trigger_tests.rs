use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::super::context::FleetCtx;
use super::user_prompt_trigger::evaluate_user_prompt_triggers;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::steerer::SteererConfig;
use crate::llm::OpenRouterClient;
use crate::steerer::registry::SteererRegistry;
use crate::store::RequestStore;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

const LOOP_WAKEUP_PROMPT: &str = "check your mailbox and continue";
const GENUINE_PROMPT: &str = "please add a retry to the http client";

async fn ledger_row_count(pool: &sqlx::PgPool, session_uuid: uuid::Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory.ask_ledger_entries e
           JOIN sessions.sessions s ON s.id = e.session_id
          WHERE s.session_uuid = $1",
    )
    .bind(session_uuid)
    .fetch_one(pool)
    .await?)
}

async fn run_user_prompt_trigger(prompt: &str) -> anyhow::Result<Option<i64>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('/tmp/user-prompt-guard', 'g', NOW(), NOW()) RETURNING id",
    )
    .fetch_one(handle.pool())
    .await?;
    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(session_uuid)
    .bind(project_id)
    .execute(handle.pool())
    .await?;

    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let mut request = crate::store::tests::make_test_request(
        Some(RequestId::from_raw(700_001)),
        &session_uuid.to_string(),
    );
    request.turn_origin = "primary".to_string();
    store.write().await.insert(request);

    let llm = OpenRouterClient::new(
        "sk-test".to_string(),
        "test-model".to_string(),
        Some("http://127.0.0.1:1".to_string()),
        None,
    )?;
    let ctx = FleetCtx {
        event_tx: tokio::sync::broadcast::channel(16).0,
        store,
        persist: Some(handle.clone()),
        steerer_registry: Arc::new(ArcSwap::from_pointee(SteererRegistry::default())),
        llm,
        key_slot: Arc::new(ArcSwap::from_pointee(Some(secrecy::SecretString::new(
            "sk-test".to_string().into(),
        )))),
        toggles: Arc::new(ArcSwap::from_pointee(
            crate::feature_toggles::FeatureToggles::default(),
        )),
        config: SteererConfig::default(),
        shutdown: CancellationToken::new(),
        remote_control_streams: crate::proxy::remote_control::ReplyStreams::default(),
        injection_gate: Arc::new(crate::injection::InjectionGate::default()),
        settings: Arc::new(ArcSwap::from_pointee(
            crate::settings::SettingsOverlay::default(),
        )),
    };
    let mut debounce = HashMap::new();
    let mut breaker = CircuitBreaker::new(10, Duration::from_secs(30), Duration::from_secs(600));
    evaluate_user_prompt_triggers(
        700_001,
        session_uuid.to_string(),
        prompt.to_string(),
        &ctx,
        &mut debounce,
        &mut breaker,
    )
    .await;

    let rows = ledger_row_count(handle.pool(), session_uuid).await?;
    db.cleanup().await?;
    Ok(Some(rows))
}

#[tokio::test]
async fn user_prompt_trigger_skips_loop_wakeup_template_and_appends_no_ledger_row()
-> anyhow::Result<()> {
    let Some(rows) = run_user_prompt_trigger(LOOP_WAKEUP_PROMPT).await? else {
        return Ok(());
    };
    assert_eq!(
        rows, 0,
        "a loop-wakeup template must not enter the ask ledger"
    );
    Ok(())
}

#[tokio::test]
async fn user_prompt_trigger_appends_ledger_row_for_genuine_prompt() -> anyhow::Result<()> {
    let Some(rows) = run_user_prompt_trigger(GENUINE_PROMPT).await? else {
        return Ok(());
    };
    assert_eq!(rows, 1, "a genuine prompt must enter the ask ledger");
    Ok(())
}
