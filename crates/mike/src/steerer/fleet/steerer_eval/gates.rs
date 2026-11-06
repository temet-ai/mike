use std::collections::HashMap;

use tracing::{debug, warn};

use super::done_claim::{contains_bounded, has_done_claim_wording};
use crate::steerer::fleet::context::{DebounceLedger, EvalInputs, EvalTarget, FleetCtx};
use crate::steerer::registry::Steerer;
use crate::store::persistence::PersistError;

pub(super) fn invocation_gate_passed(
    ctx: &FleetCtx,
    steerer: &Steerer,
    eval_inputs: &EvalInputs,
) -> bool {
    if ctx.config.force_fire {
        return true;
    }
    match steerer.invocation.as_str() {
        "every_turn" => true,
        "pattern" => pattern_gate_admits(steerer, eval_inputs),
        other => {
            debug!(
                invocation = other,
                steerer_id = steerer.id,
                "steerer fleet: unknown invocation — skipping"
            );
            false
        }
    }
}

pub(in crate::steerer::fleet) fn pattern_gate_admits(
    steerer: &Steerer,
    eval_inputs: &EvalInputs,
) -> bool {
    let pattern = steerer.pattern.as_deref().unwrap_or_default();
    pattern_invocation_matches(pattern, eval_inputs)
        || (steerer.kind == "verifier" && has_done_claim_wording(&eval_inputs.eval_text))
}

pub(super) fn pattern_invocation_matches(pattern: &str, eval_inputs: &EvalInputs) -> bool {
    pattern_matches(pattern, &eval_inputs.eval_text)
        || pattern_matches(pattern, &eval_inputs.tool_calls_summary)
}

pub(super) fn pattern_matches(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() || text.is_empty() {
        return false;
    }
    let lower_text = text.to_lowercase();
    pattern
        .lines()
        .map(str::trim)
        .filter(|kw| !kw.is_empty())
        .any(|kw| contains_bounded(&lower_text, &kw.to_lowercase()))
}

pub(super) fn debounce_allows_fire(
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    session_id: &str,
    steerer: &Steerer,
) -> bool {
    debounce
        .entry((session_id.to_owned(), steerer.id))
        .or_default()
        .check_and_record_fire("_", steerer.params.debounce_turns)
}

fn log_already_fired_for_turn(target: &EvalTarget, steerer_id: i64, turn_id: i64) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        turn_id,
        "steerer fleet: already fired for this operator turn — skipping"
    );
}

fn log_one_fire_check_failed(target: &EvalTarget, steerer_id: i64, turn_id: i64, e: &PersistError) {
    warn!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        steerer_id,
        turn_id,
        err = %e,
        "steerer fleet: one-fire-per-turn check failed — failing open to evaluate"
    );
}

pub(super) async fn already_fired_for_turn(
    ctx: &FleetCtx,
    eval_inputs: &EvalInputs,
    steerer: &Steerer,
    target: &EvalTarget,
) -> bool {
    let (Some(turn_id), Some(ph)) = (eval_inputs.judged_turn_id, ctx.persist.as_ref()) else {
        return false;
    };
    match ph.has_intervention_for_turn(steerer.id, turn_id).await {
        Ok(true) => {
            log_already_fired_for_turn(target, steerer.id, turn_id);
            true
        }
        Ok(false) => false,
        Err(e) => {
            log_one_fire_check_failed(target, steerer.id, turn_id, &e);
            false
        }
    }
}
