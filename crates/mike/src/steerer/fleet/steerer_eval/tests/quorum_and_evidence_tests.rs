use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::super::*;
use super::support::{make_verifier_steerer, mock_openrouter_returning};
use crate::steerer::config::SteererConfig;
use crate::steerer::registry::{Steerer, SteererRegistry};
use crate::store::RequestStore;
use crate::store::retention::RetentionPolicy;

#[tokio::test]
async fn low_confidence_verdict_is_declined_before_quorum() -> anyhow::Result<()> {
    let verdict_json = r#"{
            "fires": true,
            "confidence": 0.4,
            "reason": "claims completion but tests were never run",
            "nudge": "run the test suite before claiming done",
            "checklist": [
                {"criterion": "tests pass", "status": "not_done", "evidence": "req 123"}
            ]
        }"#;
    let (openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let session_uuid = uuid::Uuid::new_v4();
    let ctx = make_fleet_ctx(store.clone(), llm);

    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        "All done, everything works now.",
    )
    .await;

    assert_eq!(
        openrouter_mock
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        1,
        "confidence (0.4) below the steerer's confidence_threshold (0.7) must abstain before \
             any quorum sample is spent"
    );
    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert_eq!(
        reminders.len(),
        0,
        "a verdict that never clears the confidence threshold must never fire"
    );
    Ok(())
}

#[tokio::test]
async fn threshold_confidence_verdict_fires_once_quorum_agrees() -> anyhow::Result<()> {
    let verdict_json = r#"{
            "fires": true,
            "confidence": 0.7,
            "reason": "claims completion but tests were never run",
            "nudge": "run the test suite before claiming done",
            "checklist": [
                {"criterion": "tests pass", "status": "not_done", "evidence": "req 123"}
            ]
        }"#;
    let (openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let session_uuid = uuid::Uuid::new_v4();
    let ctx = make_fleet_ctx(store.clone(), llm);

    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        "All done, everything works now.",
    )
    .await;

    assert_eq!(
        openrouter_mock
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        3,
        "confidence exactly at the threshold (0.7) must clear the gate and be confirmed by \
             2 more independent samples (3 total)"
    );
    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert_eq!(
        reminders.len(),
        1,
        "2-of-3 quorum agreement must fire once the per-sample confidence gate is cleared"
    );
    Ok(())
}

#[tokio::test]
async fn mid_task_progress_turn_does_not_fire() -> anyhow::Result<()> {
    let verdict_json = r#"{
            "fires": false,
            "confidence": 0.9,
            "reason": "work visibly in flight, no completion claim",
            "nudge": "",
            "checklist": [
                {"criterion": "refactor auth module", "status": "partial", "evidence": "req 240600"}
            ]
        }"#;
    let (openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let session_uuid = uuid::Uuid::new_v4();
    let ctx = make_fleet_ctx(store.clone(), llm);

    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        "Still working through the refactor, three files left to update.",
    )
    .await;

    assert_eq!(
        openrouter_mock
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        1,
        "a genuine no-fire must cost exactly one call — no quorum confirmation on a \
             candidate that was never a candidate"
    );
    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert!(
        reminders.is_empty(),
        "truthful mid-task progress must not fire"
    );
    Ok(())
}

#[tokio::test]
async fn unevidenced_not_done_checklist_item_abstains_without_confirmation_calls()
-> anyhow::Result<()> {
    let verdict_json = r#"{
            "fires": true,
            "confidence": 0.95,
            "reason": "claims the criterion is not done",
            "nudge": "go check it",
            "checklist": [
                {"criterion": "tests pass", "status": "not_done", "evidence": ""}
            ]
        }"#;
    let (openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let session_uuid = uuid::Uuid::new_v4();
    let ctx = make_fleet_ctx(store.clone(), llm);

    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        "Still working through the refactor, a few files left.",
    )
    .await;

    assert_eq!(
        openrouter_mock
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        1,
        "evidence-gated abstention must short-circuit before any quorum confirmation call"
    );
    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert!(
        reminders.is_empty(),
        "an unevidenced not_done claim must abstain, not fire"
    );
    Ok(())
}

fn make_fleet_ctx(store: Arc<RwLock<RequestStore>>, llm: OpenRouterClient) -> FleetCtx {
    FleetCtx {
        event_tx: tokio::sync::broadcast::channel(16).0,
        store,
        persist: None,
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

async fn evaluate_turn(ctx: &FleetCtx, steerer: &Steerer, session_id: String, eval_text: &str) {
    let eval_inputs = EvalInputs {
        eval_text: eval_text.to_string(),
        operator_ask: "please refactor the auth module".to_string(),
        last_user_message: "please refactor the auth module".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    let mut debounce = HashMap::new();
    let mut llm_outcome = LlmCallOutcome::default();
    let target = EvalTarget {
        raw_id: 1,
        trigger_request_id: Some(1),
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
}
