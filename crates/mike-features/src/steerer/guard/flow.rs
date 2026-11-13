use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::HashMap;

use crate::logged::ok_or_warn;
use serde_json::json;
use uuid::Uuid;

use super::call::{CallFailure, ask_with_quorum, decide};
use super::context::{ContextRequest, TurnContext, load_context};
use super::exempt::{ExemptInput, question_exemption};
use super::input::{InputParts, build_input};
use super::live::{LiveFire, plan_live_fires, record_arms};
use super::policy::{GuardConfig, GuardSettings, QuestionMode, guard_config, question_modes};
use super::question_gates::{GateContext, gate};
use super::reach::reach_at;
use super::record::GuardRecord;
use super::{GuardStep, GuardTurn, Question, Trigger};
use crate::dossier::view::AsOf;
use crate::llm::LlmCallContext;
use crate::store::persistence::steering_evaluations::{
    EvaluationMode, EvaluationVerdict, NewEvaluation,
};

type Rows = Vec<(Question, Uuid)>;

pub struct GuardOutcome {
    pub step: GuardStep,
    pub fires: Vec<LiveFire>,
    rows: Rows,
    record: GuardRecord,
}

impl GuardOutcome {
    fn quiet(step: GuardStep) -> Self {
        Self {
            step,
            fires: Vec::new(),
            rows: Vec::new(),
            record: GuardRecord::default(),
        }
    }
}

pub async fn evaluate(turn: &GuardTurn<'_>) -> GuardOutcome {
    run(turn).await
}

pub async fn evaluate_idle(turn: &GuardTurn<'_>) -> GuardOutcome {
    run(turn).await
}

pub async fn finish(
    turn: &GuardTurn<'_>,
    outcome: GuardOutcome,
    delivered: &HashMap<Question, Uuid>,
) -> GuardStep {
    resolve(turn, &outcome.rows, &outcome.record, delivered).await;
    record_arms(turn.persist, &outcome.fires).await;
    outcome.step
}

async fn asked_questions(turn: &GuardTurn<'_>) -> Vec<(Question, QuestionMode)> {
    let modes = question_modes(turn.persist, turn.steerer_id).await;
    let candidates: &[Question] = match turn.trigger {
        Trigger::TurnEnd => &Question::ALL,
        Trigger::Quiescence => &[Question::UnbackedWait],
    };
    candidates
        .iter()
        .map(|q| (*q, modes.get(q).copied().unwrap_or(QuestionMode::Shadow)))
        .filter(|(_, m)| *m != QuestionMode::Off)
        .collect()
}

async fn dispatch(turn: &GuardTurn<'_>, asked: &[(Question, QuestionMode)]) -> Rows {
    let mut rows = Vec::new();
    for mode in [EvaluationMode::Shadow, EvaluationMode::Live] {
        let questions: Vec<Question> = asked
            .iter()
            .filter(|(_, m)| m.is_live() == (mode == EvaluationMode::Live))
            .map(|(q, _)| *q)
            .collect();
        rows.extend(dispatch_mode(turn, &questions, mode).await);
    }
    rows
}

async fn dispatch_mode(turn: &GuardTurn<'_>, questions: &[Question], mode: EvaluationMode) -> Rows {
    let codes: Vec<Option<String>> = questions
        .iter()
        .map(|q| Some(q.code().to_owned()))
        .collect();
    let new = NewEvaluation {
        steerer_id: turn.steerer_id,
        session_id: turn.session_db_id,
        operator_turn_id: turn.operator_turn_id,
        mode,
        bundle_id: turn.bundle_id,
        trigger_point: Some(turn.trigger.code()),
        trigger_request_id: turn.trigger_request_id,
    };
    match turn
        .persist
        .steerer()
        .insert_evaluations_dispatched(&new, &codes)
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|(uuid, q)| Some((Question::parse(q.as_deref()?)?, uuid)))
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "turn guard: evaluation dispatch failed");
            Vec::new()
        }
    }
}

async fn resolve(
    turn: &GuardTurn<'_>,
    rows: &Rows,
    record: &GuardRecord,
    delivered: &HashMap<Question, Uuid>,
) {
    for (question, uuid) in rows {
        let decision = record.decisions.get(question);
        let fired = decision.is_some_and(|d| d.fires);
        let reason = decision
            .and_then(|d| d.reason.clone())
            .or_else(|| record.abstentions.get(question).cloned());
        let verdict = EvaluationVerdict {
            verdict_json: record.row_json(*question),
            confidence: decision.and_then(|d| d.confidence),
            reason,
        };
        if let Err(e) = turn
            .persist
            .steerer()
            .resolve_evaluation(
                *uuid,
                fired,
                delivered.get(question).copied(),
                Some(verdict),
            )
            .await
        {
            tracing::warn!(evaluation = %uuid, error = %e, "turn guard: evaluation resolve failed");
        }
    }
}

fn abstain_all(record: &mut GuardRecord, questions: &[Question], reason: &str) {
    for q in questions {
        record.abstentions.insert(*q, reason.to_owned());
    }
}

pub fn recall_sample(turn_id: i64, question: Question, pct: i64) -> bool {
    if pct <= 0 {
        return false;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in turn_id
        .to_le_bytes()
        .iter()
        .chain(question.code().as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash % 100) < u64::try_from(pct).unwrap_or(0)
}

fn new_record(turn: &GuardTurn<'_>) -> GuardRecord {
    GuardRecord {
        trigger: Some(turn.trigger),
        actor: json!({
            "trigger_request_id": turn.trigger_request_id,
            "agent_instance_id": turn.agent_id,
            "subagent_persona": turn.persona,
        }),
        ..GuardRecord::default()
    }
}

async fn idle_seconds(turn: &GuardTurn<'_>) -> Option<i64> {
    let last = turn
        .persist
        .steerer()
        .session_last_request_at(turn.session_db_id)
        .await
        .ok()
        .flatten()?;
    Some((turn.now - last).num_seconds().max(0))
}

struct Gated {
    open: Vec<Question>,
    step_class: Option<&'static str>,
}

fn apply_gates(
    turn: &GuardTurn<'_>,
    ctx: &TurnContext,
    cfg: &GuardConfig,
    pct: i64,
    questions: &[Question],
    record: &mut GuardRecord,
) -> Gated {
    let gate_ctx = GateContext {
        phrase: &ctx.phrase,
        trigger: turn.trigger,
        facts: ctx.facts.as_ref(),
        prior: &ctx.prior,
        open_turn_id: turn.operator_turn_id,
        turn_judge_live: turn.turn_judge_live,
        last_tool: ctx.last_tool.as_deref(),
        directives: &ctx.directives,
        idle_seconds: record.idle_seconds,
        quiescence_after_secs: cfg.quiescence_after_secs,
        ending: &ctx.ending,
    };
    let mut gated = Gated {
        open: Vec::new(),
        step_class: None,
    };
    for &q in questions {
        let g = gate(q, &gate_ctx);
        record.gates.insert(q, g.outcome.to_json());
        if q == Question::Relapse {
            record.relapse_anchor_turn_id = g.relapse_anchor;
        }
        if g.outcome.open {
            record.abstentions.remove(&q);
            gated.open.push(q);
            gated.step_class = gated.step_class.or(g.step_class.map(|c| c.code()));
        } else {
            record
                .abstentions
                .insert(q, format!("gate_closed:{}", g.outcome.reason));
            record
                .recall_sample
                .insert(q, recall_sample(turn.operator_turn_id, q, pct));
        }
    }
    gated
}

fn input_text(ctx: &TurnContext, record: &GuardRecord, gated: &Gated) -> String {
    let correction = record.relapse_anchor_turn_id.and_then(|anchor| {
        ctx.inputs
            .turns
            .iter()
            .find(|t| t.turn_id == anchor)
            .map(|t| (anchor, t.created_at.to_rfc3339(), t.prompt.clone()))
    });
    let rationale = ctx
        .prior
        .iter()
        .rev()
        .find(|p| p.reaction.as_deref() == Some("corrected"))
        .and_then(|p| p.rationale.clone());
    build_input(&InputParts {
        ending: &ctx.ending,
        open: gated.open.clone(),
        correction,
        correction_rationale: rationale,
        directives: &ctx.directives,
        asked_class: gated.step_class,
        facts: ctx.facts.as_ref(),
        trajectory: None,
        goal: ctx.inputs.goals.as_ref().map(|g| g.goal.as_str()),
        dossier: None,
    })
}

async fn call_and_decide(
    turn: &GuardTurn<'_>,
    ctx: &TurnContext,
    cfg: &GuardConfig,
    gated: &Gated,
    record: &mut GuardRecord,
) -> GuardStep {
    let snapshot = crate::cross_session::current();
    let project_id = turn
        .persist
        .steerer()
        .session_project_id(turn.session_db_id)
        .await
        .ok()
        .flatten();
    let choices = super::threshold_select::choose(&snapshot, cfg, &gated.open, project_id);
    let open: Vec<(Question, f64)> = gated
        .open
        .iter()
        .map(|q| (*q, choices.get(q).map_or(cfg.threshold(*q), |c| c.value)))
        .collect();
    let block = super::calibration_input::block_for(&snapshot, &open, project_id, turn.now);
    let input =
        super::calibration_input::with_block(&input_text(ctx, record, gated), block.as_ref());
    let system =
        super::calibration_input::system_prompt(super::call::SYSTEM_PROMPT, block.as_ref());
    record.input_chars = Some(input.chars().count());
    record.calibration =
        Some(json!({"thresholds": super::threshold_select::to_json(&choices), "block": block}));
    let llm = turn.llm.with_call_context(LlmCallContext {
        session_id: Some(turn.session_db_id),
        steerer_id: Some(turn.steerer_id),
        operator_turn_id: Some(turn.operator_turn_id),
    });
    let threshold = |q: Question| choices.get(&q).map_or(cfg.threshold(q), |c| c.value);
    let request = super::call::CallRequest {
        system: &system,
        input: &input,
        open: &gated.open,
    };
    match ask_with_quorum(&llm, &request, &threshold).await {
        Ok(votes) => {
            for q in &gated.open {
                let d = decide(*q, &votes, threshold(*q));
                if let Some(a) = d.abstention {
                    record.abstentions.insert(*q, a.to_owned());
                }
                record.decisions.insert(*q, d);
            }
            record.votes = votes;
            GuardStep::Continue
        }
        Err(failure) => {
            abstain_all(record, &gated.open, failure.abstention());
            if matches!(failure, CallFailure::Refused) {
                GuardStep::StopWindow
            } else {
                GuardStep::Continue
            }
        }
    }
}

async fn run(turn: &GuardTurn<'_>) -> GuardOutcome {
    if !turn.toggle_on {
        return GuardOutcome::quiet(GuardStep::Continue);
    }
    let asked = asked_questions(turn).await;
    if asked.is_empty() {
        return GuardOutcome::quiet(GuardStep::Continue);
    }
    let questions: Vec<Question> = asked.iter().map(|(q, _)| *q).collect();
    let rows = dispatch(turn, &asked).await;
    let mut record = new_record(turn);
    abstain_all(&mut record, &questions, "pending");
    let mut fires = Vec::new();
    let step = judge(turn, &questions, &mut record, &mut fires).await;
    for fire in &mut fires {
        fire.evaluation = rows
            .iter()
            .find(|(q, _)| *q == fire.question)
            .map(|(_, u)| *u);
        record.live.insert(fire.question, fire.decision);
    }
    GuardOutcome {
        step,
        fires,
        rows,
        record,
    }
}

async fn calls_today(turn: &GuardTurn<'_>) -> i64 {
    ok_or_warn(
        turn.persist
            .steerer()
            .steerer_calls_today(turn.steerer_id, "steerer")
            .await,
        "turn_guard.calls_today",
    )
    .unwrap_or(i64::MAX)
}

fn exempt_split(
    turn: &GuardTurn<'_>,
    ctx: &TurnContext,
    questions: &[Question],
    fleet_seats: bool,
    record: &mut GuardRecord,
) -> Vec<Question> {
    let input = ExemptInput {
        agent_id: turn.agent_id.as_deref(),
        session_class: ctx
            .facts
            .as_ref()
            .and_then(|f| f.session_class.as_ref())
            .map(|c| c.class),
        opened_by: ctx.opened_by.as_deref(),
        open_prompt: ctx.open_prompt.as_deref(),
    };
    let mut open = Vec::new();
    let mut reason = None;
    for q in questions {
        match question_exemption(&input, *q, fleet_seats) {
            Some(r) => {
                record.abstentions.insert(*q, format!("exempt_{r}"));
                reason = Some(r);
            }
            None => open.push(*q),
        }
    }
    if open.is_empty() {
        record.exemption = reason;
    }
    open
}

async fn judge(
    turn: &GuardTurn<'_>,
    questions: &[Question],
    record: &mut GuardRecord,
    fires: &mut Vec<LiveFire>,
) -> GuardStep {
    let request = ContextRequest {
        persist: turn.persist,
        session_db_id: turn.session_db_id,
        turn_id: turn.operator_turn_id,
        as_of: AsOf::knowledge(turn.now),
        fallback_text: &turn.text,
        live_request_id: turn.trigger_request_id,
        verifier_pattern: turn.verifier_pattern.as_deref().unwrap_or(""),
    };
    let Some(mut ctx) = ok_or_warn(load_context(&request).await, "turn_guard.context") else {
        abstain_all(record, questions, "context_unavailable");
        return GuardStep::Continue;
    };
    if let Some(facts) = ctx.facts.as_mut() {
        crate::dossier::steering_facts::committed::mark_session_changes(
            turn.persist,
            turn.session_uuid,
            &mut facts.observation,
        )
        .await;
    }
    record.facts = ctx.facts.clone();
    record.reach = Some(
        reach_at(
            turn.persist,
            turn.session_db_id,
            turn.session_uuid,
            turn.now,
            true,
        )
        .await,
    );
    let settings = GuardSettings::load(turn.persist).await;
    let open = exempt_split(turn, &ctx, questions, settings.fleet_seats, record);
    if open.is_empty() {
        return GuardStep::Continue;
    }
    if turn.trigger == Trigger::Quiescence {
        record.idle_seconds = idle_seconds(turn).await;
    }
    let cfg = guard_config(turn.persist, turn.steerer_id).await;
    let gated = apply_gates(turn, &ctx, &cfg, settings.recall_sample_pct, &open, record);
    if open.contains(&Question::Relapse) {
        record.relapse_semantic = super::semantic_evidence::record(
            turn.persist,
            turn.session_db_id,
            ctx.open_prompt.as_deref().unwrap_or(""),
            &ctx.ending,
            record.relapse_anchor_turn_id,
            turn.now,
        )
        .await;
    }
    if gated.open.is_empty() {
        return GuardStep::Continue;
    }
    if calls_today(turn).await >= settings.max_calls_per_day {
        abstain_all(record, &gated.open, "daily_cap");
        return GuardStep::Continue;
    }
    let step = call_and_decide(turn, &ctx, &cfg, &gated, record).await;
    fires.extend(plan_live_fires(turn, &ctx, record).await);
    step
}
