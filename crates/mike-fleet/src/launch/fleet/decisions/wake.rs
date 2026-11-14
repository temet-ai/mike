use std::collections::BTreeMap;

use serde_json::{Value, json};
use uuid::Uuid;

use super::config::{Mode, Point};
use super::{FleetDecisions, text};
use crate::base_state::BaseState;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::seated::Seated;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

pub const QUESTION_SET: &str = "fleet_wake_check_v1";
pub const HOLD_ABOVE: f64 = 0.6;
const MESSAGE_TAIL_BYTES: usize = 1500;
const BRIEF_HEAD_BYTES: usize = 600;
const FILES_SHOWN: usize = 30;
const WAKE_QUESTION: &str = "wake";

#[derive(Debug, Clone)]
pub struct Mail<'a> {
    pub to: Uuid,
    pub from: Option<Uuid>,
    pub in_reply_to_id: Option<i64>,
    pub body: &'a str,
    pub sender_label: Option<&'a str>,
}

pub struct SeatContext {
    pub def: FleetDefinition,
    pub seat: String,
    pub task: String,
    pub mode: Mode,
    pub fleet_id: i64,
}

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        WAKE_QUESTION.to_owned(),
        choice(
            "Should this message wake the seat now, or can it wait for the seat's next turn?",
            &[
                (
                    "wake",
                    "The seat must see this now: it asks the seat for something, answers it, changes what the seat is doing, or concerns the files the seat owns.",
                ),
                (
                    "not_yet",
                    "The message is about this fleet's work but not about the seat's task, so the seat can read it with its next turn.",
                ),
                (
                    "unrelated",
                    "The message is not about the seat's task or files at all.",
                ),
            ],
        ),
    )])
}

pub fn state(seat: &SeatContext, mail: &Mail<'_>) -> Value {
    let task = seat.def.tasks.get(&seat.task);
    let files: Vec<&String> = task
        .map(|t| t.files.iter().take(FILES_SHOWN).collect())
        .unwrap_or_default();
    json!({
        "seat_task": task.map(|t| t.name.as_str()),
        "seat_task_brief": task.map(|t| text::head(&t.brief, BRIEF_HEAD_BYTES)),
        "seat_owned_files": files,
        "message_from": mail.sender_label,
        "message": text::tail(mail.body, MESSAGE_TAIL_BYTES),
    })
}

pub fn compose(asked: &Asked) -> (bool, f64) {
    if asked.failed() {
        return (false, 1.0);
    }
    match asked.choice(WAKE_QUESTION) {
        Some((key, p)) if key != "wake" && p >= HOLD_ABOVE => (true, p),
        Some((_, p)) => (false, p),
        None => (false, 1.0),
    }
}

pub async fn seat_context(persist: &PersistHandle, to: Uuid) -> Option<SeatContext> {
    let row = persist.fleet().find_fleet_for_session(to).await.ok()??;
    if row.state != "running" {
        return None;
    }
    let run = row.run_id.clone()?;
    let def = FleetDefinition::from_value(&row.definition).ok()?;
    let seated = Seated::of_run(persist, &run, &def).await;
    let task = seated.task_of(to)?.to_owned();
    let seat = seated.seat(to)?.name.clone();
    let mode = def.session_decision_mode(Point::WakeCheck, &seated, to);
    Some(SeatContext {
        def,
        seat,
        task,
        mode,
        fleet_id: row.id,
    })
}

fn from_fleet_control(def: &FleetDefinition, from: Uuid) -> bool {
    let named = [&def.coordinator, &def.acceptance]
        .into_iter()
        .filter_map(|seat| seat.session.as_deref()?.trim().parse::<Uuid>().ok());
    let mut control = named;
    control.any(|session| session == from)
}

async fn answers_own_message(persist: &PersistHandle, mail: &Mail<'_>) -> bool {
    let Some(reply_to) = mail.in_reply_to_id else {
        return false;
    };
    let Ok(Some(to_id)) = persist.resolve_session_db_id(&mail.to.to_string()).await else {
        return false;
    };
    persist
        .launch()
        .reply_answers_session(reply_to, to_id)
        .await
        .unwrap_or(true)
}

pub async fn hold(state: &BaseState, mail: &Mail<'_>) -> bool {
    let Some(persist) = state.persist() else {
        return false;
    };
    hold_with(persist, FleetDecisions::from_state(state).as_ref(), mail).await
}

pub async fn hold_with(
    persist: &PersistHandle,
    decisions: Option<&FleetDecisions>,
    mail: &Mail<'_>,
) -> bool {
    let Some(from) = mail.from else {
        return false;
    };
    if crate::mailbox_served::mailbox_served(&mail.to.to_string()) {
        return false;
    }
    let Some(seat) = seat_context(persist, mail.to).await else {
        return false;
    };
    if from_fleet_control(&seat.def, from) || seat.mode == Mode::Off {
        return false;
    }
    let Some(decider) = decisions.and_then(|d| d.decider(Point::WakeCheck, seat.mode)) else {
        return false;
    };
    if answers_own_message(persist, mail).await {
        return false;
    }
    judge(&decider, &seat, mail, persist).await
}

async fn judge(
    decider: &Decider,
    seat: &SeatContext,
    mail: &Mail<'_>,
    persist: &PersistHandle,
) -> bool {
    let session_id = persist
        .resolve_session_db_id(&mail.to.to_string())
        .await
        .ok()
        .flatten();
    let subject = DecisionSubject {
        session_id,
        fleet_run_id: Some(seat.fleet_id),
        seat: Some(seat.seat.clone()),
        ..DecisionSubject::default()
    };
    let asked = decider
        .ask(
            QUESTION_SET,
            state(seat, mail),
            questions(),
            subject,
            json!({"hold_above": HOLD_ABOVE}),
        )
        .await;
    let (held, score) = compose(&asked);
    let held = held && decider.applies();
    let applied = if held {
        "held"
    } else if decider.applies() && !asked.failed() {
        "sent"
    } else {
        "none"
    };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    held
}

pub async fn counts_document(persist: &PersistHandle, fleet_id: i64) -> Value {
    let rows = crate::llm::decisions::store(persist)
        .applied_counts_by_seat(fleet_id, Point::WakeCheck.name())
        .await
        .unwrap_or_default();
    let mut seats: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for row in rows {
        seats
            .entry(row.seat)
            .or_default()
            .insert(row.applied, row.count);
    }
    json!({
        "basis": "wake checks answered by Jev, per seat: `held` waited for the seat's next turn, `sent` woke the seat; mail from the operator or the system, replies to the seat's own messages and mail from the fleet's control seats are never held and are not counted",
        "by_seat": seats,
    })
}

pub async fn honouring(state: &BaseState, session: Uuid) -> bool {
    let Some(persist) = state.persist() else {
        return false;
    };
    honouring_with(
        persist,
        state.feature_toggle_overlay.load().decisions,
        session,
    )
    .await
}

pub async fn honouring_with(persist: &PersistHandle, layer_on: bool, session: Uuid) -> bool {
    if !layer_on {
        return false;
    }
    seat_context(persist, session)
        .await
        .is_some_and(|seat| seat.mode == Mode::On)
}

#[cfg(test)]
#[path = "wake_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "wake_db_tests.rs"]
mod db_tests;
