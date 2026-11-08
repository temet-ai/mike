#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::context::{EvalInputs, EvalTarget, FleetCtx, SteererSelection};
use super::input_budget::{CAPPED_ACTION_LIST_HEADING, CAPPED_ASK_HEADING, VerifierInputBudget};
use super::steerer_eval::{Abstention, LlmCallOutcome, run_steerer_loop};
use super::verifier_judge::VerifierJudgeContext;
use crate::llm::OpenRouterClient;
use crate::steerer::config::SteererConfig;
use crate::steerer::machine_text;
use crate::steerer::registry::SteererRegistry;
use crate::steerer::verdict::{SteererVerdict, steerer_verdict_schema};
use crate::store::RequestStore;
use crate::store::models::SystemReminder;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::EnrolmentRow;

pub(crate) struct JudgeDeps<'a> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) steerer_registry: &'a ArcSwap<SteererRegistry>,
    pub(crate) llm: &'a OpenRouterClient,
    pub(crate) key_slot: &'a crate::llm::key::OpenRouterKeySlot,
    pub(crate) config: &'a SteererConfig,
    pub(crate) settings: &'a crate::settings::SharedSettingsOverlay,
}

pub(crate) struct JudgeTurnCloseNowOutcome {
    pub(crate) fired: Option<(uuid::Uuid, SystemReminder)>,
    pub(crate) llm_call_failed: bool,
    pub(crate) llm_call_refused: bool,
}

fn not_judged(reason: Abstention) -> JudgeTurnCloseNowOutcome {
    tracing::debug!(abstention = reason.code(), "turn-close judge: not judged");
    JudgeTurnCloseNowOutcome {
        fired: None,
        llm_call_failed: false,
        llm_call_refused: false,
    }
}

pub(crate) async fn judge_turn_close_now(
    deps: JudgeDeps<'_>,
    session_id: &str,
    open_turn_id: i64,
    live_text: String,
    stripped_human_prompt: &str,
    trigger_request_id: Option<i64>,
) -> JudgeTurnCloseNowOutcome {
    let JudgeDeps {
        persist,
        steerer_registry,
        llm,
        key_slot,
        config,
        settings,
    } = deps;

    if machine_text::classify(stripped_human_prompt).is_some() {
        return not_judged(Abstention::MachineText);
    }

    let Ok(session_uuid) = session_id.parse::<uuid::Uuid>() else {
        return not_judged(Abstention::InvalidSessionId);
    };
    let registry = steerer_registry.load_full();
    let Ok(enrolments) = persist
        .list_active_enrolments_for_session(session_uuid)
        .await
        .inspect_err(|e| {
            warn!(
                session_id,
                open_turn_id,
                error = %e,
                "judge_turn_close_now: enrolment lookup failed — skipping"
            );
        })
    else {
        return not_judged(Abstention::EnrolmentLookupFailed);
    };
    let enrolled_map: HashMap<i64, EnrolmentRow> =
        enrolments.into_iter().map(|e| (e.steerer_id, e)).collect();
    let steerers_to_evaluate: Vec<&crate::steerer::registry::Steerer> = registry
        .enabled()
        .filter(|s| s.trigger_point == "turn_closed" && enrolled_map.contains_key(&s.id))
        .collect();
    if steerers_to_evaluate.is_empty() {
        return not_judged(Abstention::NoTurnClosedSteerer);
    }

    let eval_inputs = EvalInputs {
        eval_text: live_text,
        last_user_message: stripped_human_prompt.to_owned(),
        trigger_point: "turn_closed",
        turn_origin: "primary".to_string(),
        judged_turn_id: Some(open_turn_id),
        evaluation_turn_id: Some(open_turn_id),
        ..Default::default()
    };

    let ctx = turn_close_fleet_ctx(&JudgeDeps {
        persist,
        steerer_registry,
        llm,
        key_slot,
        config,
        settings,
    });
    let target = EvalTarget {
        raw_id: open_turn_id as u64,
        trigger_request_id,
        session_id: session_id.to_owned(),
    };
    let selection = SteererSelection {
        steerers: steerers_to_evaluate,
        enrolment_by_steerer_id: Some(enrolled_map),
    };
    run_turn_close_window(&ctx, &target, &eval_inputs, &selection).await
}

pub(super) fn turn_close_fleet_ctx(deps: &JudgeDeps<'_>) -> FleetCtx {
    FleetCtx {
        event_tx: tokio::sync::broadcast::channel(1).0,
        store: Arc::new(RwLock::new(RequestStore::new(
            crate::store::retention::RetentionPolicy::default(),
        ))),
        persist: Some(deps.persist.clone()),
        steerer_registry: Arc::new(ArcSwap::from_pointee(SteererRegistry::default())),
        llm: deps.llm.clone(),
        key_slot: deps.key_slot.clone(),
        toggles: Arc::new(ArcSwap::from_pointee(
            crate::feature_toggles::FeatureToggles::default(),
        )),
        config: deps.config.clone(),
        shutdown: CancellationToken::new(),
        remote_control_streams: crate::proxy::remote_control::ReplyStreams::default(),
        injection_gate: Arc::new(crate::injection::InjectionGate::default()),
        settings: deps.settings.clone(),
    }
}

pub(super) async fn run_turn_close_window(
    ctx: &FleetCtx,
    target: &EvalTarget,
    eval_inputs: &EvalInputs,
    selection: &SteererSelection<'_>,
) -> JudgeTurnCloseNowOutcome {
    let mut throwaway_debounce = HashMap::new();
    let mut llm_outcome = LlmCallOutcome::default();
    let fired = run_steerer_loop(
        target,
        eval_inputs,
        selection,
        ctx,
        &mut throwaway_debounce,
        &mut llm_outcome,
    )
    .await;
    JudgeTurnCloseNowOutcome {
        fired,
        llm_call_failed: llm_outcome.failed,
        llm_call_refused: llm_outcome.refused,
    }
}

pub(super) fn apply_evidence_gate(verdict: &mut SteererVerdict) {
    if verdict.checklist.is_empty() {
        return;
    }
    let mut evidenced_not_done = false;
    for item in &mut verdict.checklist {
        if item.evidence.trim().is_empty() {
            item.status = "not_run".to_string();
        } else if item.status == "not_done" {
            evidenced_not_done = true;
        }
    }
    if !evidenced_not_done {
        verdict.fires = false;
    }
}

pub(super) async fn confirm_quorum(
    llm: &OpenRouterClient,
    system: &str,
    user_msg: &str,
) -> (usize, usize) {
    let schema = steerer_verdict_schema();
    let (r2, r3) = tokio::join!(
        llm.verdict::<SteererVerdict>(system, user_msg, schema.clone()),
        llm.verdict::<SteererVerdict>(system, user_msg, schema),
    );
    let mut votes_fire = 1usize;
    let mut votes_total = 1usize;
    for r in [r2, r3] {
        match r {
            Ok(mut v) => {
                apply_evidence_gate(&mut v);
                votes_total += 1;
                if v.fires {
                    votes_fire += 1;
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "steerer fleet: quorum confirmation call failed — counted as no-fire"
                );
            }
        }
    }
    (votes_fire, votes_total)
}

pub(super) fn build_judge_user_message(
    inputs: &EvalInputs,
    judge_context: Option<&VerifierJudgeContext>,
    budget: VerifierInputBudget,
) -> String {
    let budgeted = budget.apply(inputs);
    let mut parts: Vec<String> = Vec::with_capacity(8);

    if !inputs.operator_ask.is_empty() {
        parts.push(format!("User ask:\n{}", budgeted.operator_ask));
    }
    if !inputs.last_user_message.is_empty() && inputs.last_user_message != inputs.operator_ask {
        parts.push(format!("Latest message:\n{}", budgeted.last_user_message));
    }
    match inputs.turn_origin.as_str() {
        "subagent" => {
            let persona = inputs
                .subagent_persona
                .as_deref()
                .unwrap_or("unknown specialist");
            parts.push(format!("Acting agent: subagent · {persona}"));
        }
        "primary" | "primary_first" => {
            parts.push("Acting agent: main orchestrator (not a specialist subagent)".to_string());
        }
        _ => {}
    }
    if !inputs.tool_calls_summary.is_empty() {
        parts.push(format!(
            "Tool calls this turn:\n{}",
            budgeted.tool_calls_summary
        ));
    }
    let label = if inputs.trigger_point == "user_prompt" {
        "User prompt to evaluate:"
    } else {
        "Assistant response to evaluate:"
    };
    if !inputs.eval_text.is_empty() {
        parts.push(format!("{label}\n{}", budgeted.eval_text));
    }
    if let Some((state_of_play, next)) = &budgeted.anchor {
        parts.push(format!(
            "Session context:\nState of play: {state_of_play}\nNext: {next}"
        ));
    }
    if let Some(diagnostics) = &budgeted.diagnostics {
        parts.push(format!("Analysis findings:\n{diagnostics}"));
    }
    if let Some(judge_context) = judge_context {
        push_judge_context_parts(&mut parts, judge_context, budget);
    }
    parts.join("\n\n")
}

fn push_judge_context_parts(
    parts: &mut Vec<String>,
    judge_context: &VerifierJudgeContext,
    budget: VerifierInputBudget,
) {
    if !judge_context.accumulated_ask.is_empty() {
        let heading = if budget.is_capped() {
            CAPPED_ASK_HEADING
        } else {
            "Accumulated ask (full accreted intent across this work-thread, oldest first):"
        };
        parts.push(format!("{heading}\n{}", judge_context.accumulated_ask));
    }
    if !judge_context.trajectory_digest.is_empty() {
        parts.push(format!(
            "Trajectory digest (this turn's tool calls, causally ordered, no-op runs compressed):\n{}",
            judge_context.trajectory_digest
        ));
    }
    if !judge_context.session_action_list.is_empty() {
        let heading = if budget.is_capped() {
            CAPPED_ACTION_LIST_HEADING
        } else {
            "Session action list (issue #158 — every tool call this session has made, \
             oldest first, unabridged; use this rather than the turn-scoped trajectory \
             digest above when judging whether the SESSION as a whole paid its debts):"
        };
        parts.push(format!("{heading}\n{}", judge_context.session_action_list));
    }
    if !judge_context.prior_fires_digest.is_empty() {
        parts.push(format!(
            "Prior fires this session on this steerer (fire memory) — only fire again if \
             the trajectory digest shows genuinely NEW evidence beyond these; do not \
             re-fire on the same already-flagged gap:\n{}",
            judge_context.prior_fires_digest
        ));
    }
    match judge_context.thread_criteria.as_deref() {
        Some(criteria) => {
            parts.push(format!(
                "Operator-approved acceptance criteria for this work-thread (issue #119) \
                 — use THESE as `checklist`'s source, one item per criterion, verbatim \
                 where reasonable; do not invent additional criteria from the accumulated \
                 ask above:\n{criteria}"
            ));
            parts.push(
                "Populate `checklist` from the criteria above: one item per criterion, \
                 each with a status (done/partial/not_done) and an evidence pointer (a \
                 'req <id>' / 'tool <id>' citation) drawn from the trajectory digest. \
                 Leave `evidence` empty when the digest genuinely does not address a \
                 criterion rather than guessing — a `not_done` claim with no citation is \
                 discarded as abstained, not counted as a failure. Base `fires` and \
                 `reason` on checklist progress, not on the last message alone — work \
                 visibly in flight per the digest must not be judged as a false \
                 completion claim."
                    .to_string(),
            );
        }
        None if judge_context.intake_criteria.is_some() => {
            let criteria = judge_context.intake_criteria.as_deref().unwrap_or_default();
            parts.push(format!(
                "Acceptance criteria authored at intake from the operator's own words \
                 (issue #152) — use THESE as `checklist`'s source, one item per \
                 criterion; do not invent additional criteria from the accumulated ask \
                 above. A criterion marked [inferred] was NOT stated by the operator: \
                 weigh it as a reasonable expectation, never as an instruction, and do \
                 not fire on an inferred criterion alone.\n{criteria}"
            ));
            parts.push(
                "Populate `checklist` from the criteria above: one item per criterion, \
                 each with a status (done/partial/not_done) and an evidence pointer (a \
                 'req <id>' / 'tool <id>' citation) drawn from the trajectory digest. \
                 Leave `evidence` empty when the digest genuinely does not address a \
                 criterion rather than guessing — a `not_done` claim with no citation is \
                 discarded as abstained, not counted as a failure. Base `fires` and \
                 `reason` on checklist progress, not on the last message alone — work \
                 visibly in flight per the digest must not be judged as a false \
                 completion claim."
                    .to_string(),
            );
        }
        None => {
            parts.push(
                "Using the accumulated ask above, decompose it into 5-15 checkable \
                 criteria and populate `checklist`: one item per criterion, each with a \
                 status (done/partial/not_done) and an evidence pointer (a 'req <id>' / \
                 'tool <id>' citation) drawn from the trajectory digest. Leave `evidence` \
                 empty when the digest genuinely does not address a criterion rather than \
                 guessing — a `not_done` claim with no citation is discarded as abstained, \
                 not counted as a failure. Base `fires` and `reason` on checklist \
                 progress, not on the last message alone — work visibly in flight per the \
                 digest must not be judged as a false completion claim."
                    .to_string(),
            );
        }
    }
}
