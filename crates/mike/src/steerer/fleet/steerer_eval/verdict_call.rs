use std::collections::HashMap;

use tracing::warn;

use super::record::EvaluationScope;
use crate::llm::{LlmCallContext, LlmError, OpenRouterClient};
use crate::steerer::fleet::context::{DebounceLedger, EvalTarget};
use crate::steerer::registry::Steerer;
use crate::steerer::verdict::{SteererVerdict, steerer_verdict_schema};

pub(super) fn resolve_effective_client(
    base_llm: &OpenRouterClient,
    steerer: &Steerer,
    scope: Option<&EvaluationScope>,
) -> OpenRouterClient {
    let model_resolved = if steerer.model.is_empty() {
        base_llm.clone()
    } else {
        match base_llm.with_model(steerer.model.clone()) {
            Ok(c) => c,
            Err(e) => {
                warn!(err = %e, model = %steerer.model, "steerer fleet: model override failed — using fleet default");
                base_llm.clone()
            }
        }
    };
    model_resolved.with_call_context(LlmCallContext {
        session_id: scope.map(|s| s.session_db_id),
        steerer_id: Some(steerer.id),
        operator_turn_id: scope.map(|s| s.operator_turn_id),
    })
}

pub(super) enum VerdictAttempt {
    Success(SteererVerdict),
    Refused,
    Failed,
}

fn log_verdict_call_refused(target: &EvalTarget, steerer_id: i64, e: &LlmError) {
    warn!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        err = %e,
        "steerer fleet: evaluation window stopped — call governor refused (ceiling/rate limit)"
    );
}

fn log_verdict_call_failed(target: &EvalTarget, steerer_id: i64, e: &LlmError) {
    warn!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        err = %e,
        "steerer fleet: LLM verdict error — skipping steerer (fail-closed)"
    );
}

fn allow_debounce_retry(
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    target: &EvalTarget,
    steerer_id: i64,
) {
    if let Some(ledger) = debounce.get_mut(&(target.session_id.clone(), steerer_id)) {
        ledger.allow_retry();
    }
}

pub(super) async fn attempt_verdict(
    effective_llm: &OpenRouterClient,
    system: &str,
    user_msg: &str,
    target: &EvalTarget,
    steerer: &Steerer,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
) -> VerdictAttempt {
    match effective_llm
        .verdict::<SteererVerdict>(system, user_msg, steerer_verdict_schema())
        .await
    {
        Ok(v) => VerdictAttempt::Success(v),
        Err(e) if e.is_refusal() => {
            log_verdict_call_refused(target, steerer.id, &e);
            allow_debounce_retry(debounce, target, steerer.id);
            VerdictAttempt::Refused
        }
        Err(e) => {
            log_verdict_call_failed(target, steerer.id, &e);
            allow_debounce_retry(debounce, target, steerer.id);
            VerdictAttempt::Failed
        }
    }
}
