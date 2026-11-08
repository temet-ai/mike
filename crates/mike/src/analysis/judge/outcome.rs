use std::time::Duration;

use super::{JudgeFailure, LlmError, MAX_TRANSIENT_ATTEMPTS, backoff_for_attempt, is_transient};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JudgeOutcome {
    Failed(JudgeFailure),
    Refused,
    CredentialFailure,
}

pub(super) fn log_and_refuse(turn_id: i64, attempt: u32, e: &LlmError) -> JudgeOutcome {
    tracing::warn!(
        turn_id,
        attempt,
        error = %e,
        "judge: LLM call refused by our own call governor — not attempted, turn left untouched"
    );
    JudgeOutcome::Refused
}

pub(super) fn log_and_credential_failure(turn_id: i64, attempt: u32, e: &LlmError) -> JudgeOutcome {
    tracing::warn!(
        turn_id,
        attempt,
        error = %e,
        "judge: OpenRouter credential unusable — turn left untouched, not a fact about this turn"
    );
    JudgeOutcome::CredentialFailure
}

pub(super) async fn log_and_backoff(
    turn_id: i64,
    attempt: u32,
    e: &LlmError,
    backoff_base: Duration,
) {
    let delay = backoff_for_attempt(attempt, backoff_base);
    tracing::warn!(
        turn_id,
        attempt,
        max_attempts = MAX_TRANSIENT_ATTEMPTS,
        delay_ms = delay.as_millis(),
        error = %e,
        "judge: transient LLM error — retrying"
    );
    sleep(delay).await;
}

pub(super) fn log_and_fail(turn_id: i64, attempt: u32, e: &LlmError) -> JudgeOutcome {
    let transient = is_transient(e);
    tracing::warn!(
        turn_id,
        attempt,
        transient,
        error = %e,
        "judge: LLM verdict failed — leaving outcome unknown"
    );
    JudgeOutcome::Failed(if transient {
        JudgeFailure::TransientExhausted
    } else {
        JudgeFailure::Permanent
    })
}
