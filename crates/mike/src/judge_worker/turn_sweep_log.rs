use std::time::Duration;

use crate::analysis::judge::JudgeLabel;
use crate::store::persistence::PersistError;

pub(super) fn log_turn_session_lookup_failed(turn_id: i64, e: &PersistError) {
    tracing::warn!(turn_id, error = %e, "judge_worker: session lookup for llm call attribution failed");
}

pub(super) fn log_sweep_deferred(defer: Duration) {
    tracing::debug!(
        defer_ms = defer.as_millis(),
        "judge_worker: no llm budget available — sweep deferred, nothing claimed"
    );
}

pub(super) fn log_sweep_breaker_open() {
    tracing::debug!("judge_worker: circuit breaker open — skipping sweep");
}

pub(super) fn log_sweep_probing(turn_id: i64) {
    tracing::info!(turn_id, "judge_worker: circuit breaker probing recovery");
}

pub(super) fn log_sweep_stopped_refused(turn_id: i64) {
    tracing::warn!(
        turn_id,
        "judge_worker: sweep stopped — call governor refused (ceiling/rate limit), turn left untouched"
    );
}

pub(super) fn log_sweep_stopped_credential_failure(turn_id: i64) {
    tracing::warn!(
        turn_id,
        "judge_worker: sweep stopped — OpenRouter credential unusable, turn left untouched"
    );
}

pub(super) fn log_mark_judge_failed_failed(turn_id: i64, e: &PersistError) {
    tracing::warn!(turn_id, error = %e, "judge: mark_judge_failed failed");
}

pub(super) fn log_sweep_tripped(turn_id: i64) {
    tracing::warn!(
        turn_id,
        "judge_worker: circuit breaker tripped — stopping sweep early"
    );
}

pub(super) fn log_below_threshold(
    turn_id: i64,
    label: &JudgeLabel,
    confidence: f32,
    threshold: f32,
) {
    tracing::debug!(
        turn_id,
        label = ?label,
        confidence,
        threshold,
        "judge: confidence below threshold — stamping unknown"
    );
}

pub(super) fn log_stamped(turn_id: i64, outcome: &str, confidence: f32) {
    tracing::debug!(turn_id, outcome, confidence, "judge: stamped turn");
}

pub(super) fn log_already_stamped(turn_id: i64, verdict_id: i64) {
    tracing::debug!(
        turn_id,
        verdict_id,
        "judge: turn already stamped (race), verdict kept unprojected"
    );
}

pub(super) fn log_stamp_failed(turn_id: i64, e: &PersistError) {
    tracing::warn!(turn_id, error = %e, "judge: live verdict insert and stamp failed");
}
