use super::delivery_gate::delivery_gate;
use super::gates::{already_fired_for_turn, debounce_allows_fire};
use super::log::{
    log_confidence_declined, log_debounced, log_quorum_not_reached, log_verdict_declined,
};
use super::verdict_call::{VerdictAttempt, attempt_verdict, resolve_effective_client};
use super::{Abstention, EvalWindow, Exit, StepOutcome, WindowState};
use crate::llm::OpenRouterClient;
use crate::steerer::fleet::input_budget::VerifierInputBudget;
use crate::steerer::fleet::intervention::DeterministicOverrides;
use crate::steerer::fleet::judge::{apply_evidence_gate, build_judge_user_message, confirm_quorum};
use crate::steerer::fleet::session_block::{BlockRequest, session_block, with_session_block};
use crate::steerer::fleet::verifier_arming::{
    ensure_verifier_context_fallback, verifier_arming_check,
};
use crate::steerer::note_policy::{self, NoteLimits, SessionNotes, VerdictStop};
use crate::steerer::refocus::resolve_verdict_system_prompt;
use crate::steerer::registry::Steerer;
use crate::steerer::verdict::SteererVerdict;
use crate::store::persistence::PersistHandle;

pub struct ResolvedVerdict {
    pub verdict: SteererVerdict,
    pub effective_llm: OpenRouterClient,
    pub system: String,
    pub user_msg: String,
}

pub fn gate_decline_reason(steerer: &Steerer) -> Abstention {
    if steerer.invocation == "pattern" {
        Abstention::PatternGateDeclined
    } else {
        Abstention::UnknownInvocation
    }
}

async fn build_user_message(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    state: &mut WindowState<'_>,
) -> String {
    let ctx = window.ctx;
    ensure_verifier_context_fallback(
        ctx,
        steerer,
        window.eval_inputs,
        window.session_uuid,
        &mut state.verifier_context,
    )
    .await;
    let judge_context = if steerer.kind == "verifier" {
        state.verifier_context.as_ref()
    } else {
        None
    };
    let budget = judge_context.map_or_else(
        || VerifierInputBudget::for_steerer_kind(&ctx.settings, &steerer.kind),
        |c| c.budget,
    );
    let message = build_judge_user_message(window.eval_inputs, judge_context, budget);
    with_session_block(message, &window_session_block(window, state).await)
}

fn note_limits(window: &EvalWindow<'_>) -> NoteLimits {
    NoteLimits::from_settings(&window.ctx.settings.load())
}

async fn window_session_block(window: &EvalWindow<'_>, state: &mut WindowState<'_>) -> String {
    if let Some(block) = &state.session_block {
        return block.clone();
    }
    let (Some(persist), Some(scope)) = (window.ctx.persist.as_ref(), window.scope()) else {
        return String::new();
    };
    let request = BlockRequest::now(
        scope.session_db_id,
        scope.operator_turn_id,
        note_limits(window).window,
    );
    let block = session_block(persist, &request).await;
    state.session_block = Some(block.clone());
    block
}

async fn load_session_notes(window: &EvalWindow<'_>, state: &mut WindowState<'_>) {
    if state.session_notes.is_some() {
        return;
    }
    let Some(persist) = window.ctx.persist.as_ref() else {
        return;
    };
    let Some(session_db_id) = session_db_id_of(window, persist).await else {
        return;
    };
    let notes = SessionNotes::load(
        persist,
        session_db_id,
        note_limits(window),
        chrono::Utc::now(),
    )
    .await;
    state.session_notes = Some(notes);
}

async fn session_db_id_of(window: &EvalWindow<'_>, persist: &PersistHandle) -> Option<i64> {
    if let Some(scope) = window.scope() {
        return Some(scope.session_db_id);
    }
    persist
        .resolve_session_db_id(&window.target.session_id)
        .await
        .ok()
        .flatten()
}

pub async fn resolve_verdict_for_steerer(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    state: &mut WindowState<'_>,
) -> Result<ResolvedVerdict, Exit> {
    let system = resolve_verdict_system_prompt(
        window.ctx.persist.as_ref(),
        steerer.agent_id,
        steerer.prompt.as_deref(),
    )
    .await;
    let user_msg = build_user_message(window, steerer, state).await;
    let effective_llm = resolve_effective_client(window.base_llm, steerer, window.scope());

    let attempt = attempt_verdict(
        &effective_llm,
        &system,
        &user_msg,
        window.target,
        steerer,
        state.debounce,
    )
    .await;
    let mut verdict = match attempt {
        VerdictAttempt::Success(v) => v,
        VerdictAttempt::Refused => {
            state.outcome.refused = true;
            state.outcome.abstain(steerer, Abstention::GovernorRefused);
            return Err(Exit::not_judged(
                StepOutcome::StopWindow,
                Abstention::GovernorRefused,
            ));
        }
        VerdictAttempt::Failed => {
            state.outcome.failed = true;
            state.outcome.abstain(steerer, Abstention::LlmCallFailed);
            return Err(Exit::not_judged(
                StepOutcome::Continue,
                Abstention::LlmCallFailed,
            ));
        }
    };
    apply_evidence_gate(&mut verdict);
    state.outcome.last_verdict = Some(verdict.clone());

    Ok(ResolvedVerdict {
        verdict,
        effective_llm,
        system,
        user_msg,
    })
}

pub async fn pre_verdict_decline(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    state: &mut WindowState<'_>,
) -> Option<Abstention> {
    let ctx = window.ctx;
    let target = window.target;

    if !ctx.config.force_fire {
        load_session_notes(window, state).await;
        let refused = note_policy::before_judging(state.session_notes.as_ref(), steerer.id);
        if let Some(refusal) = refused {
            return Some(Abstention::Note(refusal));
        }
    }
    if let Some(reason) = delivery_gate(ctx, steerer, window.scope()).await {
        return Some(reason);
    }
    if !ctx.config.force_fire && !debounce_allows_fire(state.debounce, &target.session_id, steerer)
    {
        log_debounced(target, steerer.id);
        return Some(Abstention::Debounced);
    }
    if already_fired_for_turn(ctx, window.eval_inputs, steerer, target).await {
        return Some(Abstention::AlreadyFiredForTurn);
    }
    if verifier_arming_check(
        ctx,
        steerer,
        window.eval_inputs,
        window.session_uuid,
        &mut state.verifier_context,
        target,
    )
    .await
    {
        return Some(Abstention::ArmingSuppressed);
    }
    None
}

pub async fn post_verdict_decline(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    resolved: &ResolvedVerdict,
    overrides: &DeterministicOverrides,
    notes: Option<&SessionNotes>,
) -> Option<Abstention> {
    let target = window.target;
    let verdict = &resolved.verdict;
    let forced = overrides.any() || window.ctx.config.force_fire;

    let threshold = crate::cross_session::effective::effective_fire_threshold(
        &crate::cross_session::current(),
        steerer.id,
        steerer.params.confidence_threshold,
    );
    match note_policy::stop_before_quorum(verdict, f64::from(threshold), notes) {
        Some(VerdictStop::Declined) if !overrides.any() => {
            log_verdict_declined(target, steerer.id, verdict);
            return Some(Abstention::VerdictDeclined);
        }
        Some(VerdictStop::ConfidenceBelowThreshold) if !forced => {
            log_confidence_declined(target, steerer.id, verdict, threshold);
            return Some(Abstention::ConfidenceBelowThreshold);
        }
        Some(VerdictStop::Note(refusal)) if !forced => {
            return Some(Abstention::Note(refusal));
        }
        _ => {}
    }
    if !forced {
        let (votes_fire, votes_total) = confirm_quorum(
            &resolved.effective_llm,
            &resolved.system,
            &resolved.user_msg,
        )
        .await;
        if votes_fire < 2 {
            log_quorum_not_reached(target, steerer.id, votes_fire, votes_total);
            return Some(Abstention::QuorumNotReached);
        }
    }
    None
}
