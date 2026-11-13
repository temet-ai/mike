use crate::store::persistence::steerer_repo::SteererPersist as _;
use chrono::{DateTime, Duration, Utc};

use super::context::TurnContext;
use super::live_decision::{
    AnchorHistory, LiveContext, LiveDecision, decide_live_fire, highest_live,
};
use super::nudges::{NudgeInputs, render_nudge};
use super::policy::{GuardSettings, question_policies};
use super::record::GuardRecord;
use super::{GuardTurn, Question, Trigger};
use crate::steerer::note_policy::{self, SessionNotes};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::promotion::ArmWrite;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveFire {
    pub(crate) question: Question,
    pub(crate) decision: LiveDecision,
    pub(crate) nudge: String,
    pub(crate) evaluation: Option<uuid::Uuid>,
}

impl LiveFire {
    pub(crate) fn delivers(&self) -> bool {
        matches!(self.decision, LiveDecision::Deliver { .. })
    }
}

fn anchor_of(record: &GuardRecord, question: Question, turn_id: i64) -> i64 {
    if question == Question::Relapse {
        record.relapse_anchor_turn_id.unwrap_or(turn_id)
    } else {
        turn_id
    }
}

async fn same_change_fires(turn: &GuardTurn<'_>, question: Question, change: Option<i64>) -> usize {
    let Some(change) = change.filter(|_| question == Question::UnobservedDone) else {
        return 0;
    };
    let count = turn
        .persist
        .steerer()
        .same_change_fires(turn.steerer_id, question.code(), turn.session_db_id, change)
        .await
        .unwrap_or_default();
    usize::try_from(count).unwrap_or_default()
}

async fn prior_for(
    turn: &GuardTurn<'_>,
    question: Question,
    anchor: i64,
    change: Option<i64>,
) -> AnchorHistory {
    let fires = turn
        .persist
        .steerer()
        .anchor_history(turn.steerer_id, question.code(), anchor)
        .await
        .unwrap_or_default();
    AnchorHistory {
        treated_pending_or_delivered: fires
            .iter()
            .filter(|f| f.delivered || f.wake_state.as_deref() == Some("pending"))
            .count(),
        same_change_fires: same_change_fires(turn, question, change).await,
    }
}

fn turn_end_at(turn: &GuardTurn<'_>, record: &GuardRecord) -> DateTime<Utc> {
    match (turn.trigger, record.idle_seconds) {
        (Trigger::Quiescence, Some(idle)) => turn.now - Duration::seconds(idle),
        _ => turn.now,
    }
}

fn nudge_for(question: Question, ctx: &TurnContext, record: &GuardRecord, anchor: i64) -> String {
    let quote = record
        .votes
        .first()
        .and_then(|v| v.answer(question))
        .map_or("", |a| a.quote.as_str());
    let correction = ctx
        .inputs
        .turns
        .iter()
        .find(|t| t.turn_id == anchor)
        .map_or("", |t| t.prompt.as_str());
    let observation = ctx.facts.as_ref().map(|f| &f.observation);
    let change_tool = observation
        .and_then(|o| o.last_change_use_id)
        .map(|id| format!("tool {id}"))
        .unwrap_or_default();
    let change_at = observation
        .and_then(|o| o.last_change_at)
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();
    let directive = ctx.directives.iter().find(|d| d.polarity == "allow");
    let refs = ctx
        .facts
        .as_ref()
        .map(|f| f.wait_backing_refs.as_slice())
        .unwrap_or_default();
    let authorised_at = directive
        .and_then(|d| d.operator_turn_id)
        .map(|id| format!("turn {id}"))
        .unwrap_or_default();
    render_nudge(
        question,
        &NudgeInputs {
            correction_text: correction,
            evidence_quote: quote,
            ending_quote: quote,
            open_items: refs.iter().filter(|r| r.source == "work_item").count(),
            awaited_replies: refs.iter().filter(|r| r.source == "peer_reply").count(),
            last_change_tool: &change_tool,
            last_change_at: &change_at,
            authorisation_class: directive.map_or("", |d| d.class.as_str()),
            authorisation_text: directive.map_or("", |d| d.text.as_str()),
            authorised_at: &authorised_at,
        },
    )
}

async fn session_notes(turn: &GuardTurn<'_>) -> Option<SessionNotes> {
    let limits = turn.note_limits?;
    Some(SessionNotes::load(turn.persist, turn.session_db_id, limits, turn.now).await)
}

pub(crate) async fn plan_live_fires(
    turn: &GuardTurn<'_>,
    ctx: &TurnContext,
    record: &GuardRecord,
) -> Vec<LiveFire> {
    let policies = question_policies(turn.persist, turn.steerer_id).await;
    let firing_live: Vec<Question> = record
        .decisions
        .values()
        .filter(|d| d.fires)
        .map(|d| d.question)
        .filter(|q| policies.get(q).is_some_and(|p| p.mode.is_live()))
        .collect();
    if firing_live.is_empty() {
        return Vec::new();
    }
    let settings = GuardSettings::load(turn.persist).await;
    let class = ctx
        .facts
        .as_ref()
        .and_then(|f| f.session_class.as_ref())
        .map_or("unknown", |c| c.class);
    let higher = highest_live(&firing_live);
    let notes = session_notes(turn).await;
    let mut fires = Vec::new();
    for question in firing_live {
        let Some(policy) = policies.get(&question) else {
            continue;
        };
        let anchor = anchor_of(record, question, turn.operator_turn_id);
        let change = ctx
            .facts
            .as_ref()
            .and_then(|f| f.observation.last_change_use_id);
        let prior = prior_for(turn, question, anchor, change).await;
        let live = LiveContext {
            question,
            steerer_id: turn.steerer_id,
            anchor_turn_id: anchor,
            policy,
            session_class: class,
            note_refusal: note_policy::refusal(notes.as_ref(), turn.steerer_id, question.need()),
            higher_live: higher,
            prior: &prior,
            turn_end_at: turn_end_at(turn, record),
            now: turn.now,
        };
        fires.push(LiveFire {
            question,
            decision: decide_live_fire(&live, &settings),
            nudge: nudge_for(question, ctx, record, anchor),
            evaluation: None,
        });
    }
    fires
}

fn arm_write(fire: &LiveFire) -> Option<ArmWrite> {
    let evaluation = fire.evaluation?;
    match fire.decision {
        LiveDecision::Holdback {
            draw,
            anchor_turn_id,
            due_at,
        } => Some(ArmWrite {
            evaluation,
            arm: "holdback",
            draw,
            anchor_turn_id,
            due_at,
        }),
        LiveDecision::Deliver {
            draw,
            anchor_turn_id,
            plan,
        } => Some(ArmWrite {
            evaluation,
            arm: "treated",
            draw,
            anchor_turn_id,
            due_at: plan.deliver_after(),
        }),
        _ => None,
    }
}

pub(crate) async fn record_arms(persist: &PersistHandle, fires: &[LiveFire]) {
    for w in fires.iter().filter_map(arm_write) {
        if let Err(e) = persist.steerer().record_arm(&w).await {
            tracing::warn!(evaluation = %w.evaluation, error = %e, "turn guard: arm record failed");
        }
    }
}
