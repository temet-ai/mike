pub mod delivery_gate;
pub mod done_claim;
pub mod gate_probe;
pub mod gates;
pub mod guard_delivery;
pub mod guard_seam;
pub mod jev_note;
pub mod judgement;
pub mod log;
pub mod record;
pub mod rollout;
pub mod steps;
#[cfg(test)]
mod tests;
pub mod turn_notes;
pub mod verdict_call;
pub mod verifier_seam;

pub use gate_probe::steerer_gate_probe;

use std::collections::HashMap;

use gates::invocation_gate_passed;
pub use gates::pattern_gate_admits;

use delivery_gate::repeats_last_delivery;
pub use judgement::{Abstention, TurnJudgement};
use log::{log_no_api_key, log_shadow_would_fire, log_unknown_kind};
pub use record::EvaluationScope;
use record::{Recorder, Resolution, resolve_exit, resolve_scope};
use rollout::{is_shadow, live_before_shadow};
use steps::{
    ResolvedVerdict, gate_decline_reason, post_verdict_decline, pre_verdict_decline,
    resolve_verdict_for_steerer,
};

use super::context::{DebounceLedger, EvalInputs, EvalTarget, FleetCtx, SteererSelection};
use super::intervention::{
    FireContext, deterministic_overrides, record_and_broadcast, select_nudge_message,
};
use super::verifier_judge::VerifierJudgeContext;
use crate::llm::OpenRouterClient;
use crate::steerer::note_policy::SessionNotes;
use crate::steerer::registry::Steerer;
use crate::steerer::verdict::SteererVerdict;
use crate::store::models::SystemReminder;
#[cfg(test)]
use gates::{pattern_invocation_matches, pattern_matches};
#[cfg(test)]
use verdict_call::resolve_effective_client;

#[derive(Debug, Default, Clone)]
pub struct LlmCallOutcome {
    pub failed: bool,
    pub refused: bool,
    pub last_verdict: Option<SteererVerdict>,
    pub abstention: Option<Abstention>,
}

impl LlmCallOutcome {
    fn abstain(&mut self, steerer: &Steerer, reason: Abstention) {
        if self.abstention.is_none() || steerer.kind == "verifier" {
            self.abstention = Some(reason);
        }
    }
}

pub struct EvalWindow<'a> {
    ctx: &'a FleetCtx,
    target: &'a EvalTarget,
    eval_inputs: &'a EvalInputs,
    selection: &'a SteererSelection<'a>,
    base_llm: &'a OpenRouterClient,
    session_uuid: Option<uuid::Uuid>,
    recorder: Option<Recorder<'a>>,
}

impl EvalWindow<'_> {
    fn scope(&self) -> Option<&EvaluationScope> {
        self.recorder.as_ref().map(|r| &r.scope)
    }
}

pub struct WindowState<'s> {
    debounce: &'s mut HashMap<(String, i64), DebounceLedger>,
    verifier_context: Option<VerifierJudgeContext>,
    session_notes: Option<SessionNotes>,
    session_block: Option<String>,
    outcome: &'s mut LlmCallOutcome,
}

pub enum StepOutcome {
    Continue,
    StopWindow,
    Fired(uuid::Uuid, SystemReminder),
}

pub struct Exit {
    step: StepOutcome,
    resolution: Resolution,
}

impl Exit {
    fn not_judged(step: StepOutcome, reason: Abstention) -> Self {
        Self {
            step,
            resolution: Resolution::Pass(TurnJudgement::not_judged(reason)),
        }
    }

    fn judged(verdict: SteererVerdict, reason: Abstention) -> Self {
        Self {
            step: StepOutcome::Continue,
            resolution: Resolution::Pass(TurnJudgement {
                verdict: Some(verdict),
                abstention: Some(reason),
            }),
        }
    }

    fn would_fire(verdict: SteererVerdict) -> Self {
        Self {
            step: StepOutcome::Continue,
            resolution: Resolution::WouldFire(TurnJudgement {
                verdict: Some(verdict),
                abstention: None,
            }),
        }
    }
}

fn kind_supported(steerer: &Steerer) -> bool {
    matches!(
        steerer.kind.as_str(),
        "refocus" | "memory_keeper" | "verifier"
    )
}

async fn dispatch_rows(window: &EvalWindow<'_>, steerer: &Steerer) -> Vec<uuid::Uuid> {
    let Some(recorder) = window.recorder.as_ref() else {
        return Vec::new();
    };
    let questions = vec![window.eval_inputs.question.clone()];
    let bundle_id = window.ctx.settings.load().bundles.steerer(steerer.id);
    recorder.dispatch(steerer, &questions, bundle_id).await
}

async fn fire_steerer(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    resolved: &ResolvedVerdict,
    nudge_message: &str,
) -> Exit {
    let fire = FireContext {
        ctx: window.ctx,
        steerer,
        effective_model: resolved.effective_llm.model(),
        selection: window.selection,
        verdict: &resolved.verdict,
        target: window.target,
        eval_inputs: window.eval_inputs,
        scope: window.scope(),
    };
    let judgement = TurnJudgement {
        verdict: Some(resolved.verdict.clone()),
        abstention: None,
    };
    match record_and_broadcast(&fire, nudge_message).await {
        Some((uuid, reminder)) => Exit {
            step: StepOutcome::Fired(uuid, reminder),
            resolution: Resolution::Fired {
                intervention: uuid,
                judgement,
            },
        },
        None => Exit {
            step: StepOutcome::Continue,
            resolution: Resolution::Pass(judgement),
        },
    }
}

async fn fire_after_precheck(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    resolved: &ResolvedVerdict,
    nudge_message: &str,
    rows: &[uuid::Uuid],
) -> Exit {
    let precheck = verifier_seam::run(window, steerer, resolved, nudge_message, rows).await;
    if let Some(held) = precheck.as_ref().filter(|p| p.holds()) {
        held.mark(false).await;
        return Exit::would_fire(resolved.verdict.clone());
    }
    let exit = fire_steerer(window, steerer, resolved, nudge_message).await;
    if let Some(sent) = precheck {
        sent.mark(matches!(exit.step, StepOutcome::Fired(..))).await;
    }
    exit
}

async fn judge_past_gate(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    state: &mut WindowState<'_>,
    rows: &[uuid::Uuid],
) -> Exit {
    if let Some(reason) = pre_verdict_decline(window, steerer, state).await {
        state.outcome.abstain(steerer, reason);
        return Exit::not_judged(StepOutcome::Continue, reason);
    }
    let resolved = match resolve_verdict_for_steerer(window, steerer, state).await {
        Ok(resolved) => resolved,
        Err(exit) => return exit,
    };
    let overrides = deterministic_overrides(
        window.ctx,
        steerer,
        &resolved.verdict,
        window.session_uuid,
        window.target,
    )
    .await;
    let notes = state.session_notes.as_ref();
    if let Some(reason) = post_verdict_decline(window, steerer, &resolved, &overrides, notes).await
    {
        state.outcome.abstain(steerer, reason);
        return Exit::judged(resolved.verdict, reason);
    }
    let nudge_message = select_nudge_message(&overrides, &resolved.verdict);
    if repeats_last_delivery(window.ctx, steerer, window.scope(), &nudge_message).await {
        state
            .outcome
            .abstain(steerer, Abstention::RepeatsLastDelivery);
        return Exit::judged(resolved.verdict, Abstention::RepeatsLastDelivery);
    }
    if is_shadow(steerer) {
        log_shadow_would_fire(window.target, steerer.id);
        return Exit::would_fire(resolved.verdict);
    }
    fire_after_precheck(window, steerer, &resolved, &nudge_message, rows).await
}

async fn evaluate_one_steerer(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    state: &mut WindowState<'_>,
) -> StepOutcome {
    if window.ctx.shutdown.is_cancelled() {
        state.outcome.abstain(steerer, Abstention::ShutDown);
        return StepOutcome::StopWindow;
    }
    if steerer.kind == "turn_guard" {
        return guard_seam::evaluate_guard(window, steerer).await;
    }
    if !kind_supported(steerer) {
        log_unknown_kind(steerer);
        return StepOutcome::Continue;
    }
    if !invocation_gate_passed(window.ctx, steerer, window.eval_inputs) {
        state.outcome.abstain(steerer, gate_decline_reason(steerer));
        return StepOutcome::Continue;
    }
    let rows = dispatch_rows(window, steerer).await;
    let exit = judge_past_gate(window, steerer, state, &rows).await;
    resolve_exit(window.recorder.as_ref(), &rows, exit.resolution).await;
    exit.step
}

async fn build_recorder<'a>(
    ctx: &'a FleetCtx,
    target: &EvalTarget,
    eval_inputs: &EvalInputs,
) -> Option<Recorder<'a>> {
    let persist = ctx.persist.as_ref()?;
    let scope = resolve_scope(ctx, target, eval_inputs).await?;
    Some(Recorder {
        persist,
        scope,
        retain_pass_verdicts: ctx.settings.load().steerer_retain_pass_verdicts,
    })
}

pub async fn run_steerer_loop(
    target: &EvalTarget,
    eval_inputs: &EvalInputs,
    selection: &SteererSelection<'_>,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    outcome: &mut LlmCallOutcome,
) -> Option<(uuid::Uuid, SystemReminder)> {
    let session_uuid: Option<uuid::Uuid> = target.session_id.parse().ok();

    let Some(base_llm) = crate::llm::key::effective_client_for_any(
        &ctx.llm,
        &ctx.key_slot,
        selection.steerers.iter().map(|s| s.model.as_str()),
    ) else {
        log_no_api_key(target);
        outcome.abstention = Some(Abstention::NoApiKey);
        return None;
    };

    let window = EvalWindow {
        ctx,
        target,
        eval_inputs,
        selection,
        base_llm: &base_llm,
        session_uuid,
        recorder: build_recorder(ctx, target, eval_inputs).await,
    };
    let mut state = WindowState {
        debounce,
        verifier_context: None,
        session_notes: None,
        session_block: None,
        outcome,
    };

    for steerer in live_before_shadow(&selection.steerers) {
        match evaluate_one_steerer(&window, steerer, &mut state).await {
            StepOutcome::Continue => continue,
            StepOutcome::StopWindow => break,
            StepOutcome::Fired(uuid, reminder) => {
                state.outcome.abstention = None;
                return Some((uuid, reminder));
            }
        }
    }
    None
}
