use std::sync::{Arc, Mutex};

use super::support::{make_verifier_steerer, mock_openrouter_returning};
use super::*;
use crate::llm::{LlmCallRecord, LlmRole, LlmUsageSink, RoleSpend};

#[derive(Default)]
struct CapturingSink {
    calls: Mutex<Vec<LlmCallRecord>>,
}

impl LlmUsageSink for CapturingSink {
    fn record(&self, call: LlmCallRecord) {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    fn spent_today(&self) -> RoleSpend {
        RoleSpend::default()
    }
}

fn fleet_default_client() -> anyhow::Result<OpenRouterClient> {
    OpenRouterClient::new(
        "sk-test".to_owned(),
        "openai/gpt-5.6-luna".to_owned(),
        None,
        None,
    )
    .map_err(Into::into)
}

#[test]
fn inherited_steerer_model_resolves_to_fleet_default_model() -> anyhow::Result<()> {
    let steerer = make_verifier_steerer();
    assert!(steerer.model.is_empty());

    let effective = resolve_effective_client(&fleet_default_client()?, &steerer, None);

    assert_eq!(effective.model(), "openai/gpt-5.6-luna");
    Ok(())
}

#[test]
fn steerer_model_override_wins_over_fleet_default() -> anyhow::Result<()> {
    let mut steerer = make_verifier_steerer();
    steerer.model = "google/gemini-3.8-flash".to_owned();

    let effective = resolve_effective_client(&fleet_default_client()?, &steerer, None);

    assert_eq!(effective.model(), "google/gemini-3.8-flash");
    Ok(())
}

#[tokio::test]
async fn steerer_verdict_call_is_recorded_with_effective_model_and_steerer_id() -> anyhow::Result<()>
{
    let (_server, mock_client) = mock_openrouter_returning(r#"{"ok":true}"#).await?;
    let sink = Arc::new(CapturingSink::default());
    let sink_dyn: Arc<dyn LlmUsageSink> = sink.clone();
    let base = mock_client.with_attribution(LlmRole::Steerer, Some(&sink_dyn));
    let steerer = make_verifier_steerer();

    let effective = resolve_effective_client(&base, &steerer, None);
    effective
        .verdict::<serde_json::Value>("sys", "user", serde_json::json!({"type": "object"}))
        .await?;

    let calls = sink.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].role, LlmRole::Steerer);
    assert_eq!(calls[0].model, "test-model");
    assert_eq!(calls[0].steerer_id, Some(steerer.id));
    Ok(())
}

#[tokio::test]
async fn verdict_call_records_session_and_turn_on_llm_calls() -> anyhow::Result<()> {
    let (_server, mock_client) = mock_openrouter_returning(r#"{"ok":true}"#).await?;
    let sink = Arc::new(CapturingSink::default());
    let sink_dyn: Arc<dyn LlmUsageSink> = sink.clone();
    let base = mock_client.with_attribution(LlmRole::Steerer, Some(&sink_dyn));
    let steerer = make_verifier_steerer();
    let scope = super::super::record::EvaluationScope {
        session_db_id: 41,
        operator_turn_id: 97,
        trigger_point: "turn_end",
        trigger_request_id: Some(5),
        actor: None,
    };

    let effective = resolve_effective_client(&base, &steerer, Some(&scope));
    effective
        .verdict::<serde_json::Value>("sys", "user", serde_json::json!({"type": "object"}))
        .await?;

    let calls = sink.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].session_id, Some(41));
    assert_eq!(calls[0].operator_turn_id, Some(97));
    assert_eq!(calls[0].steerer_id, Some(steerer.id));
    Ok(())
}
