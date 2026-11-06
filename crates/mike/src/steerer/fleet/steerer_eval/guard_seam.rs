use super::{EvalWindow, StepOutcome};
use std::collections::HashMap;

use super::guard_delivery::deliver_guard_fire;
use crate::steerer::guard::{GuardStep, GuardTurn, Trigger, evaluate, evaluate_idle, finish};
use crate::steerer::note_policy::NoteLimits;
use crate::steerer::registry::Steerer;

fn verifier_pattern(window: &EvalWindow<'_>) -> Option<String> {
    let registry = window.ctx.steerer_registry.load_full();
    registry
        .enabled()
        .find(|s| s.kind == "verifier")
        .and_then(|s| s.pattern.clone())
}

pub(super) async fn evaluate_guard(window: &EvalWindow<'_>, steerer: &Steerer) -> StepOutcome {
    let (Some(persist), Some(scope)) = (window.ctx.persist.as_ref(), window.scope()) else {
        return StepOutcome::Continue;
    };
    let settings = window.ctx.settings.load();
    let trigger = if scope.trigger_point == "quiescence" {
        Trigger::Quiescence
    } else {
        Trigger::TurnEnd
    };
    let actor = scope.actor.as_ref();
    let turn = GuardTurn {
        persist,
        llm: window.base_llm,
        steerer_id: steerer.id,
        bundle_id: settings.bundles.steerer(steerer.id),
        session_db_id: scope.session_db_id,
        session_uuid: window.session_uuid,
        operator_turn_id: scope.operator_turn_id,
        trigger,
        trigger_request_id: scope.trigger_request_id,
        agent_id: actor.and_then(|a| a.agent_id.clone()),
        persona: actor.and_then(|a| a.persona.clone()),
        text: window.eval_inputs.eval_text.clone(),
        verifier_pattern: verifier_pattern(window),
        toggle_on: window.ctx.toggles.load().turn_guard,
        turn_judge_live: settings.turn_judge_live_bundle_id > 0,
        note_limits: (!window.ctx.config.force_fire).then(|| NoteLimits::from_settings(&settings)),
        now: chrono::Utc::now(),
    };
    let outcome = match trigger {
        Trigger::TurnEnd => evaluate(&turn).await,
        Trigger::Quiescence => evaluate_idle(&turn).await,
    };
    let mut delivered = HashMap::new();
    for fire in outcome.fires.iter().filter(|f| f.delivers()) {
        if let Some(uuid) = deliver_guard_fire(window, steerer, fire).await {
            delivered.insert(fire.question, uuid);
        }
    }
    let step = finish(&turn, outcome, &delivered).await;
    match step {
        GuardStep::Continue => StepOutcome::Continue,
        GuardStep::StopWindow => StepOutcome::StopWindow,
    }
}
