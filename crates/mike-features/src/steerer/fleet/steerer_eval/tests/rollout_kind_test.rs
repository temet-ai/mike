use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::support::{make_fleet_ctx, make_verifier_steerer, mock_openrouter_returning};
use crate::steerer::rollout_params::Rollout;
use crate::store::RequestStore;
use crate::store::retention::RetentionPolicy;

const FIRING: &str = r#"{"fires":true,"confidence":0.95,"reason":"claimed done without a test run","nudge":"run the tests","checklist":[]}"#;

fn steerer(id: i64, kind: &str, model: &str, rollout: Rollout) -> Steerer {
    let mut s = make_verifier_steerer();
    s.id = id;
    s.kind = kind.to_owned();
    s.model = model.to_owned();
    s.params.delivery.rollout = rollout;
    s.params.suppress_first_n_turns = 0;
    s
}

fn inputs() -> EvalInputs {
    EvalInputs {
        eval_text: "All done, shipped.".to_owned(),
        operator_ask: "finish the work".to_owned(),
        last_user_message: "finish the work".to_owned(),
        trigger_point: "response",
        ..Default::default()
    }
}

async fn run(
    ctx: &FleetCtx,
    steerers: Vec<&Steerer>,
) -> (Option<(uuid::Uuid, SystemReminder)>, LlmCallOutcome) {
    let mut debounce = HashMap::new();
    let mut outcome = LlmCallOutcome::default();
    let target = EvalTarget {
        raw_id: 1,
        trigger_request_id: None,
        session_id: uuid::Uuid::new_v4().to_string(),
    };
    let selection = SteererSelection {
        steerers,
        enrolment_by_steerer_id: None,
    };
    let fired = run_steerer_loop(
        &target,
        &inputs(),
        &selection,
        ctx,
        &mut debounce,
        &mut outcome,
    )
    .await;
    (fired, outcome)
}

async fn models_called(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).ok())
        .filter_map(|b| b["model"].as_str().map(str::to_owned))
        .collect()
}

fn fleet(llm: OpenRouterClient) -> FleetCtx {
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    make_fleet_ctx(store, None, llm.with_role(crate::llm::LlmRole::Steerer))
}

#[tokio::test]
async fn unknown_kind_steerer_makes_no_call() -> anyhow::Result<()> {
    let (server, llm) = mock_openrouter_returning(FIRING).await?;
    let ctx = fleet(llm);
    let guard = steerer(1, "turn_guard", "model-guard", Rollout::Live);
    let verifier = steerer(2, "verifier", "model-verifier", Rollout::Live);
    let (fired, _) = run(&ctx, vec![&guard, &verifier]).await;
    let models = models_called(&server).await;
    assert!(!models.iter().any(|m| m == "model-guard"), "{models:?}");
    assert!(models.iter().any(|m| m == "model-verifier"), "{models:?}");
    assert!(fired.is_some(), "the next steerer is still evaluated");
    Ok(())
}

#[tokio::test]
async fn existing_kinds_route_to_generic_path() -> anyhow::Result<()> {
    for kind in ["refocus", "memory_keeper", "verifier"] {
        crate::llm::limiter::reset_global_for_tests();
        let (server, llm) = mock_openrouter_returning(FIRING).await?;
        let ctx = fleet(llm);
        let s = steerer(1, kind, "model-generic", Rollout::Live);
        run(&ctx, vec![&s]).await;
        assert!(
            !models_called(&server).await.is_empty(),
            "{kind} made no call"
        );
    }
    Ok(())
}

#[tokio::test]
async fn shadow_would_fire_continues_to_next_steerer() -> anyhow::Result<()> {
    let (server, llm) = mock_openrouter_returning(FIRING).await?;
    let ctx = fleet(llm);
    let a = steerer(1, "refocus", "model-a", Rollout::Shadow);
    let b = steerer(2, "refocus", "model-b", Rollout::Shadow);
    let (fired, _) = run(&ctx, vec![&a, &b]).await;
    assert!(fired.is_none(), "a shadow steerer never delivers");
    let models = models_called(&server).await;
    assert!(models.iter().any(|m| m == "model-a"));
    assert!(models.iter().any(|m| m == "model-b"));
    Ok(())
}

#[tokio::test]
async fn shadow_would_fire_writes_no_reminder() -> anyhow::Result<()> {
    let (_server, llm) = mock_openrouter_returning(FIRING).await?;
    let ctx = fleet(llm);
    let a = steerer(1, "refocus", "model-a", Rollout::Shadow);
    let (fired, outcome) = run(&ctx, vec![&a]).await;
    assert!(fired.is_none());
    assert!(outcome.last_verdict.is_some_and(|v| v.fires));
    Ok(())
}

#[tokio::test]
async fn shadow_steerer_after_live_fire_is_not_evaluated() -> anyhow::Result<()> {
    let (server, llm) = mock_openrouter_returning(FIRING).await?;
    let ctx = fleet(llm);
    let shadow = steerer(1, "refocus", "model-shadow", Rollout::Shadow);
    let live = steerer(2, "refocus", "model-live", Rollout::Live);
    let (fired, _) = run(&ctx, vec![&shadow, &live]).await;
    assert!(fired.is_some(), "the live steerer runs first and fires");
    let models = models_called(&server).await;
    assert!(!models.iter().any(|m| m == "model-shadow"), "{models:?}");
    Ok(())
}

#[tokio::test]
async fn response_path_keeps_already_fired_gate_inert() -> anyhow::Result<()> {
    let (server, llm) = mock_openrouter_returning(FIRING).await?;
    let ctx = fleet(llm);
    let live = steerer(2, "verifier", "model-live", Rollout::Live);
    let mut debounce = HashMap::new();
    let mut outcome = LlmCallOutcome::default();
    let target = EvalTarget {
        raw_id: 1,
        trigger_request_id: Some(1),
        session_id: uuid::Uuid::new_v4().to_string(),
    };
    let mut response = inputs();
    response.evaluation_turn_id = Some(77);
    let selection = SteererSelection {
        steerers: vec![&live],
        enrolment_by_steerer_id: None,
    };
    run_steerer_loop(
        &target,
        &response,
        &selection,
        &ctx,
        &mut debounce,
        &mut outcome,
    )
    .await;
    assert!(response.judged_turn_id.is_none());
    assert_ne!(outcome.abstention, Some(Abstention::AlreadyFiredForTurn));
    assert!(!models_called(&server).await.is_empty());
    Ok(())
}
