use serde_json::json;
use uuid::Uuid;

use super::context::TurnContext;
use super::live::LiveFire;
use super::live_decision::LiveDecision;
use super::record::GuardRecord;
use super::{GuardTurn, Question};
use crate::dossier::steering_facts::STEERING_FACTS_VERSION;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::record::{Applied, DecisionContext, DecisionMode, DecisionSubject};
use crate::steerer::precheck::state::PrecheckInput;
use crate::steerer::precheck::{self, POINT, Precheck};

#[derive(Clone)]
pub struct DecisionsTurn {
    pub client: OpenRouterClient,
    pub mode: DecisionMode,
    pub hold_below: f64,
}

pub fn decisions_turn(
    base: &OpenRouterClient,
    persist: &crate::store::persistence::PersistHandle,
    toggle_on: bool,
    overlay: &crate::settings::SettingsOverlay,
) -> Option<DecisionsTurn> {
    if !toggle_on {
        return None;
    }
    let mode = match overlay.decisions_turn_guard_mode.as_str() {
        "on" => DecisionMode::On,
        "shadow" => DecisionMode::Shadow,
        _ => return None,
    };
    let client = crate::llm::decisions::decision_client(base, overlay)
        .ok()?
        .with_decision_store(persist.clone());
    Some(DecisionsTurn {
        client,
        mode,
        hold_below: overlay.decisions_turn_guard_hold_below,
    })
}

fn context(
    turn: &GuardTurn<'_>,
    ctx: &TurnContext,
    mode: DecisionMode,
    evaluation: Option<Uuid>,
) -> DecisionContext {
    let mut c = DecisionContext::new(POINT, mode, precheck::questions::QUESTION_SET);
    c.facts_version = Some(i32::from(STEERING_FACTS_VERSION));
    c.facts = ctx
        .facts
        .as_ref()
        .and_then(|f| serde_json::to_value(f).ok());
    c.subject = DecisionSubject {
        session_id: Some(turn.session_db_id),
        operator_turn_id: Some(turn.operator_turn_id),
        request_id: turn.trigger_request_id,
        evaluation_uuid: evaluation,
        ..DecisionSubject::default()
    };
    c
}

pub async fn mark_committed(turn: &GuardTurn<'_>, ctx: &mut TurnContext) {
    if let Some(facts) = ctx.facts.as_mut() {
        crate::dossier::steering_facts::committed::mark_session_changes(
            turn.persist,
            turn.session_uuid,
            &mut facts.observation,
        )
        .await;
    }
}

pub async fn run_prechecks(
    turn: &GuardTurn<'_>,
    ctx: &TurnContext,
    rows: &[(Question, Uuid)],
    record: &mut GuardRecord,
) {
    let Some(decisions) = turn.decisions.as_ref() else {
        return;
    };
    let firing: Vec<(Question, String)> = record
        .decisions
        .values()
        .filter(|d| d.fires && precheck::applies_to(d.question))
        .map(|d| (d.question, d.reason.clone().unwrap_or_default()))
        .collect();
    for (question, reason) in firing {
        let evaluation = rows.iter().find(|(q, _)| *q == question).map(|(_, u)| *u);
        let input = PrecheckInput {
            question,
            steerer_reason: &reason,
            operator_ask: ctx.open_prompt.as_deref().unwrap_or(""),
            final_message: &ctx.ending,
            facts: ctx.facts.as_ref(),
        };
        let c = context(turn, ctx, decisions.mode, evaluation);
        if let Some(result) =
            precheck::run(&decisions.client, &input, c, decisions.hold_below).await
        {
            record.precheck.insert(question, result);
        }
    }
}

pub fn holds(turn: &GuardTurn<'_>, record: &GuardRecord, question: Question) -> bool {
    turn.decisions
        .as_ref()
        .is_some_and(|d| d.mode == DecisionMode::On)
        && record
            .precheck
            .get(&question)
            .is_some_and(|p| !p.verdict.send && !p.fail_open)
}

pub fn held(decision: LiveDecision) -> LiveDecision {
    match decision {
        LiveDecision::Deliver {
            draw,
            anchor_turn_id,
            ..
        } => LiveDecision::Held {
            draw,
            anchor_turn_id,
        },
        other => other,
    }
}

fn applied(check: &Precheck, fire: Option<&LiveFire>) -> &'static str {
    match fire.map(|f| f.decision) {
        Some(LiveDecision::Held { .. }) => "held",
        Some(LiveDecision::Deliver { .. }) if check.verdict.send => "sent",
        _ => "none",
    }
}

pub async fn mark_applied(turn: &GuardTurn<'_>, record: &GuardRecord, fires: &[LiveFire]) {
    for (question, check) in &record.precheck {
        let Some(id) = check.record_id else {
            continue;
        };
        let fire = fires.iter().find(|f| f.question == *question);
        let mark = Applied {
            composed: json!({ "verdict": check.verdict, "answers": check.answers }),
            applied: applied(check, fire),
            fail_open: check.fail_open,
        };
        if let Err(error) = crate::llm::decisions::store(turn.persist)
            .mark_decision_applied(id, &mark, None)
            .await
        {
            tracing::warn!(%error, record = id, "turn guard: pre-check applied mark failed");
        }
    }
}
