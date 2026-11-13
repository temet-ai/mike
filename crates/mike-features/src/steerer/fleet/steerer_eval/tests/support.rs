use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::super::*;
use crate::config::steerer::SteererConfig;
use crate::steerer::registry::{Steerer, SteererParams, SteererRegistry};
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;

pub async fn seed_project_and_session(
    pool: &sqlx::PgPool,
    project_path: &str,
) -> anyhow::Result<(i64, uuid::Uuid, i64)> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        project_path,
    )
    .fetch_one(pool)
    .await?;

    let session_uuid = uuid::Uuid::new_v4();
    let session_db_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok((project_id, session_uuid, session_db_id))
}

pub async fn mock_openrouter_returning(
    verdict_json: &str,
) -> anyhow::Result<(wiremock::MockServer, OpenRouterClient)> {
    let openrouter_mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": verdict_json}}]
                }))
                .insert_header("content-type", "application/json"),
        )
        .mount(&openrouter_mock)
        .await;
    let llm = OpenRouterClient::new(
        "sk-test".to_string(),
        "test-model".to_string(),
        Some(openrouter_mock.uri()),
        None,
    )?;
    Ok((openrouter_mock, llm))
}

pub fn make_verifier_steerer() -> Steerer {
    Steerer {
        id: 1,
        uuid: uuid::Uuid::new_v4(),
        name: "verifier".to_string(),
        display_name: "Verifier".to_string(),
        description: "test".to_string(),
        agent_id: 1,
        authority: "redirect".to_string(),
        mode: "augment".to_string(),
        model: String::new(),
        priority: 0,
        enabled: true,
        params: SteererParams::default(),
        config: serde_json::json!({}),
        prompt: Some("evaluate for false completion".to_string()),
        kind: "verifier".to_string(),
        trigger_point: "response".to_string(),
        invocation: "every_turn".to_string(),
        pattern: None,
        auto_enrol: false,
        project_id: None,
        project_path: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

pub fn make_fleet_ctx(
    store: Arc<RwLock<RequestStore>>,
    persist: Option<PersistHandle>,
    llm: OpenRouterClient,
) -> FleetCtx {
    FleetCtx {
        event_tx: tokio::sync::broadcast::channel(16).0,
        store,
        persist,
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
        settings: Arc::new(arc_swap::ArcSwap::from_pointee(
            crate::settings::SettingsOverlay::default(),
        )),
    }
}

pub async fn evaluate_turn(
    ctx: &FleetCtx,
    steerer: &Steerer,
    session_id: String,
    trigger_request_id: Option<i64>,
    eval_inputs: EvalInputs,
) {
    evaluate_turn_outcome(ctx, steerer, session_id, trigger_request_id, eval_inputs).await;
}

pub async fn evaluate_turn_outcome(
    ctx: &FleetCtx,
    steerer: &Steerer,
    session_id: String,
    trigger_request_id: Option<i64>,
    eval_inputs: EvalInputs,
) -> LlmCallOutcome {
    let mut debounce = HashMap::new();
    let mut llm_outcome = LlmCallOutcome::default();
    let target = EvalTarget {
        raw_id: 1,
        trigger_request_id,
        session_id,
    };
    let selection = SteererSelection {
        steerers: vec![steerer],
        enrolment_by_steerer_id: None,
    };
    run_steerer_loop(
        &target,
        &eval_inputs,
        &selection,
        ctx,
        &mut debounce,
        &mut llm_outcome,
    )
    .await;
    llm_outcome
}
