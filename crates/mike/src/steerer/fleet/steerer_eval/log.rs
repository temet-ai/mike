use tracing::{debug, warn};

use crate::steerer::fleet::context::EvalTarget;
use crate::steerer::registry::Steerer;
use crate::steerer::verdict::SteererVerdict;

pub(super) fn log_no_api_key(target: &EvalTarget) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        "steerer fleet: no OpenRouter API key configured — skipping evaluation"
    );
}

pub(super) fn log_debounced(target: &EvalTarget, steerer_id: i64) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        "steerer fleet: debounced — suppressed repeat intervention"
    );
}

pub(super) fn log_verdict_declined(target: &EvalTarget, steerer_id: i64, verdict: &SteererVerdict) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        fires = verdict.fires,
        confidence = verdict.confidence,
        "steerer fleet: verdict declined (or evidence-gated abstention) — no intervention"
    );
}

pub(super) fn log_confidence_declined(
    target: &EvalTarget,
    steerer_id: i64,
    verdict: &SteererVerdict,
    threshold: f32,
) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        confidence = verdict.confidence,
        threshold,
        "steerer fleet: confidence below threshold — abstaining despite candidate fire"
    );
}

pub(super) fn log_quorum_not_reached(
    target: &EvalTarget,
    steerer_id: i64,
    votes_fire: usize,
    votes_total: usize,
) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        votes_fire,
        votes_total,
        "steerer fleet: 2-of-3 verdict quorum not reached — abstaining despite candidate fire"
    );
}

pub(super) fn log_unknown_kind(steerer: &Steerer) {
    warn!(
        steerer_id = steerer.id,
        kind = %steerer.kind,
        "steerer fleet: no evaluation path for this steerer kind — skipping without a call"
    );
}

pub(super) fn log_shadow_would_fire(target: &EvalTarget, steerer_id: i64) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        "steerer fleet: shadow steerer would fire — recorded, not delivered"
    );
}
