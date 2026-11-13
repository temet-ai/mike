use crate::store::persistence::steerer_repo::SteererPersist as _;
pub mod response_trigger;
#[cfg(test)]
mod tests;
pub mod user_prompt_trigger;
#[cfg(test)]
pub mod user_prompt_trigger_tests;

pub use response_trigger::evaluate_and_maybe_fire;
pub use user_prompt_trigger::evaluate_user_prompt_triggers;

use std::collections::HashMap;
use std::time::Instant;

use tracing::{debug, warn};

use super::context::{DebounceLedger, EvalInputs, EvalTarget, FleetCtx, SteererSelection};
use super::steerer_eval::{LlmCallOutcome, run_steerer_loop};
use crate::circuit_breaker::CircuitBreaker;
use crate::steerer::anchor::Anchor;
use crate::steerer::registry::{Steerer, SteererRegistry};
use crate::store::persistence::steering::EnrolmentRow;

fn is_work_lane(turn_origin: &str) -> bool {
    matches!(turn_origin, "primary" | "primary_first")
}

fn response_trigger_gate(
    trigger_point: &str,
    stop_reason: Option<&str>,
    text_non_empty: bool,
) -> bool {
    match trigger_point {
        "" | "response" => true,
        "turn_end" => stop_reason == Some("end_turn") && text_non_empty,
        _ => false,
    }
}

fn log_anchor_lookup_failed(target: &EvalTarget, e: &crate::store::persistence::PersistError) {
    warn!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        error = %e,
        "steerer fleet: anchor lookup failed — anchor unavailable"
    );
}

pub async fn fetch_anchor(
    ctx: &FleetCtx,
    session_uuid: uuid::Uuid,
    target: &EvalTarget,
) -> Option<Anchor> {
    let ph = ctx.persist.as_ref()?;
    match ph.steerer().get_session_goal(session_uuid).await {
        Ok(Some(sg)) => Some(Anchor {
            state_of_play: sg.goal,
            next: sg.next_steps,
        }),
        Ok(None) => None,
        Err(e) => {
            log_anchor_lookup_failed(target, &e);
            None
        }
    }
}

fn log_no_persistence_forces_skip(target: &EvalTarget, trigger: &str) {
    debug!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        trigger,
        "steerer fleet: no persistence — enrolment gate forces skip"
    );
}

fn log_enrolment_lookup_failed(
    target: &EvalTarget,
    trigger: &str,
    e: &crate::store::persistence::PersistError,
) {
    warn!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        trigger,
        error = %e,
        "steerer fleet: enrolment lookup failed — skipping"
    );
}

pub async fn resolve_trigger_steerers<'r>(
    ctx: &FleetCtx,
    registry: &'r SteererRegistry,
    session_uuid: uuid::Uuid,
    target: &EvalTarget,
    trigger: &str,
    gate: impl Fn(&Steerer) -> bool,
) -> Option<SteererSelection<'r>> {
    if ctx.config.force_fire {
        let steerers = registry.enabled().filter(|s| gate(s)).collect();
        return Some(SteererSelection {
            steerers,
            enrolment_by_steerer_id: None,
        });
    }
    let Some(ph) = ctx.persist.as_ref() else {
        log_no_persistence_forces_skip(target, trigger);
        return None;
    };
    let enrolments = match ph
        .steerer()
        .list_active_enrolments_for_session(session_uuid)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            log_enrolment_lookup_failed(target, trigger, &e);
            return None;
        }
    };
    let enrolled_map: HashMap<i64, EnrolmentRow> =
        enrolments.into_iter().map(|e| (e.steerer_id, e)).collect();
    let steerers = registry
        .enabled()
        .filter(|s| gate(s) && enrolled_map.contains_key(&s.id))
        .collect();
    Some(SteererSelection {
        steerers,
        enrolment_by_steerer_id: Some(enrolled_map),
    })
}

pub async fn run_loop_and_record_breaker(
    ctx: &FleetCtx,
    target: &EvalTarget,
    eval_inputs: &EvalInputs,
    selection: &SteererSelection<'_>,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    breaker: &mut CircuitBreaker,
) {
    let mut llm_outcome = LlmCallOutcome::default();
    run_steerer_loop(
        target,
        eval_inputs,
        selection,
        ctx,
        debounce,
        &mut llm_outcome,
    )
    .await;
    if !llm_outcome.refused {
        breaker.record(!llm_outcome.failed, Instant::now());
    }
}
