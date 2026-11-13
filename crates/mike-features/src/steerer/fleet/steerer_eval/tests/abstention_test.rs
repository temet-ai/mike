use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::support::{
    evaluate_turn_outcome, make_fleet_ctx, make_verifier_steerer, mock_openrouter_returning,
};
use crate::store::RequestStore;
use crate::store::retention::RetentionPolicy;

fn claim_inputs(text: &str) -> EvalInputs {
    EvalInputs {
        eval_text: text.to_string(),
        operator_ask: "finish the work".to_string(),
        last_user_message: "finish the work".to_string(),
        trigger_point: "response",
        ..Default::default()
    }
}

fn pattern_verifier(pattern: &str) -> Steerer {
    let mut steerer = make_verifier_steerer();
    steerer.invocation = "pattern".to_string();
    steerer.pattern = Some(pattern.to_string());
    steerer
}

#[tokio::test]
async fn pattern_gate_decline_is_recorded_as_an_abstention() -> anyhow::Result<()> {
    let verdict = r#"{"fires":false,"confidence":0.9,"reason":"unused","nudge":"","checklist":[]}"#;
    let (_mock, llm) = mock_openrouter_returning(verdict).await?;
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store, None, llm);

    let outcome = evaluate_turn_outcome(
        &ctx,
        &pattern_verifier(" done"),
        uuid::Uuid::new_v4().to_string(),
        None,
        claim_inputs("I abandoned that approach."),
    )
    .await;

    assert_eq!(outcome.abstention, Some(Abstention::PatternGateDeclined));
    assert!(
        outcome.last_verdict.is_none(),
        "a declined gate must not reach the LLM"
    );
    Ok(())
}

#[tokio::test]
async fn declined_verdict_is_recorded_with_the_verdict_and_its_abstention() -> anyhow::Result<()> {
    let verdict =
        r#"{"fires":false,"confidence":0.9,"reason":"the tests ran","nudge":"","checklist":[]}"#;
    let (_mock, llm) = mock_openrouter_returning(verdict).await?;
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store, None, llm);

    let outcome = evaluate_turn_outcome(
        &ctx,
        &pattern_verifier(" done"),
        uuid::Uuid::new_v4().to_string(),
        None,
        claim_inputs("The feature has shipped."),
    )
    .await;

    assert_eq!(outcome.abstention, Some(Abstention::VerdictDeclined));
    let recorded = outcome.last_verdict.ok_or_else(|| {
        anyhow::anyhow!("the verifier gate must reach the LLM for a shipped claim")
    })?;
    assert_eq!(recorded.reason, "the tests ran");
    Ok(())
}

#[tokio::test]
async fn low_confidence_candidate_fire_is_recorded_as_an_abstention() -> anyhow::Result<()> {
    let verdict =
        r#"{"fires":true,"confidence":0.2,"reason":"unsure","nudge":"check","checklist":[]}"#;
    let (_mock, llm) = mock_openrouter_returning(verdict).await?;
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store, None, llm);

    let outcome = evaluate_turn_outcome(
        &ctx,
        &make_verifier_steerer(),
        uuid::Uuid::new_v4().to_string(),
        None,
        claim_inputs("all done"),
    )
    .await;

    assert_eq!(
        outcome.abstention,
        Some(Abstention::ConfidenceBelowThreshold)
    );
    Ok(())
}
