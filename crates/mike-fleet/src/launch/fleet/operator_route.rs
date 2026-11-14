use std::collections::BTreeMap;

use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::leaves::tell_seat;
use crate::base_state::BaseState;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::tail_chars;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::session_notes::{SessionNoteRow, SessionNoteUpdate};

pub const POINT: &str = "needs_operator";
pub const QUESTION_SET: &str = "needs_operator_v1";
pub const TAG: &str = "routed-to-coordinator";
pub const ROUTE_BELOW: f64 = 0.3;
const NEEDS: &str = "needs_operator";
const BODY_CHARS: usize = 1500;

pub struct Asking<'a> {
    pub source: &'static str,
    pub seat: &'a str,
    pub title: &'a str,
    pub body: &'a str,
}

pub struct Judged {
    pub asked: Asked,
    pub needs: Option<f64>,
}

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        NEEDS.to_owned(),
        noul(
            "A seat of a fleet raised a question for the operator, the person who owns the fleet. Does the question need the operator, or could the fleet's coordinator answer it?",
            "Only the operator can answer it: it asks for a decision about scope, priorities, money, credentials, access or a ruling the fleet's rules reserve to the operator.",
            "The coordinator can answer it: it asks about the fleet's own tasks, files, seats or order of work, or about something the fleet's rules and the repository already settle.",
        ),
    )])
}

pub fn state(asking: &Asking<'_>) -> Value {
    json!({
        "raised_through": asking.source,
        "seat": asking.seat,
        "title": asking.title,
        "body_tail": tail_chars(asking.body, BODY_CHARS),
    })
}

pub fn compose(asked: &Asked) -> Option<f64> {
    if asked.failed() {
        return None;
    }
    asked.noul(NEEDS)
}

pub fn routes_to_coordinator(needs: Option<f64>) -> bool {
    needs.is_some_and(|p| p < ROUTE_BELOW)
}

pub async fn ask(decider: &Decider, asking: &Asking<'_>, subject: DecisionSubject) -> Judged {
    let asked = decider
        .ask(
            QUESTION_SET,
            state(asking),
            questions(),
            subject,
            json!({"route_below": ROUTE_BELOW}),
        )
        .await;
    let needs = compose(&asked);
    Judged { asked, needs }
}

impl Judged {
    pub fn will_route(&self, decider: &Decider) -> bool {
        decider.applies() && routes_to_coordinator(self.needs)
    }

    pub async fn finish(&self, decider: &Decider, routed: bool) {
        let applied = match (self.needs, routed) {
            (None, _) => "none",
            (Some(_), true) => "sent",
            (Some(_), false) => "let_through",
        };
        let score = self.needs.map_or(1.0, |p| if routed { 1.0 - p } else { p });
        decider
            .mark(
                &self.asked,
                self.asked.composed(json!({"score": score})),
                applied,
            )
            .await;
    }
}

async fn coordinator_of(persist: &PersistHandle, session: Uuid) -> Option<Uuid> {
    let row = persist
        .fleet()
        .find_fleet_for_session(session)
        .await
        .ok()
        .flatten()?;
    let def = FleetDefinition::from_value(&row.definition).ok()?;
    coordinator_session(&def)
}

fn coordinator_session(def: &FleetDefinition) -> Option<Uuid> {
    def.coordinator.session.as_deref()?.trim().parse().ok()
}

fn mail_body(asking: &Asking<'_>, needs: f64) -> String {
    format!(
        "[mike routing] {} raised a question for the operator that Jev judged you can answer ({:.0}% that it needs the operator): {}\n{}\nAnswer the seat yourself in a message to it. If you cannot, escalate it to the operator.",
        asking.seat,
        needs * 100.0,
        asking.title,
        asking.body
    )
}

pub fn spawn_note(state: &BaseState, session: Uuid, note: &SessionNoteRow) {
    if note.status.as_deref() != Some("operator") {
        return;
    }
    let Some(decider) = state.decision_env().and_then(|env| env.decider(POINT)) else {
        return;
    };
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let note = note.clone();
    state.tracker.spawn(async move {
        route_note(&decider, &persist, session, &note).await;
    });
}

pub async fn route_note(
    decider: &Decider,
    persist: &PersistHandle,
    session: Uuid,
    note: &SessionNoteRow,
) {
    let Some(coordinator) = coordinator_of(persist, session).await else {
        return;
    };
    if coordinator == session {
        return;
    }
    let seat = session.to_string();
    let asking = Asking {
        source: "operator note",
        seat: &seat,
        title: &note.title,
        body: &note.body,
    };
    let judged = ask(decider, &asking, DecisionSubject::default()).await;
    let routed = judged.will_route(decider)
        && tag_and_mail(persist, coordinator, note, &asking, &judged).await;
    judged.finish(decider, routed).await;
}

async fn tag_and_mail(
    persist: &PersistHandle,
    coordinator: Uuid,
    note: &SessionNoteRow,
    asking: &Asking<'_>,
    judged: &Judged,
) -> bool {
    let mut tags = note.tags.clone();
    if !tags.iter().any(|tag| tag == TAG) {
        tags.push(TAG.to_owned());
    }
    let update = SessionNoteUpdate {
        tags: Some(&tags),
        ..SessionNoteUpdate::default()
    };
    if let Err(error) = persist
        .fleet()
        .update_session_note(note.note_uuid, update)
        .await
    {
        tracing::warn!(%error, note = %note.note_uuid, "needs_operator: could not mark the note as routed");
        return false;
    }
    tell_seat(
        persist,
        coordinator,
        &mail_body(asking, judged.needs.unwrap_or(0.0)),
    )
    .await;
    true
}

pub struct Escalation<'a> {
    pub run: &'a str,
    pub problem_id: Uuid,
    pub task: Option<&'a str>,
    pub raised_by: &'a str,
    pub question: &'a str,
    pub reason: &'a str,
}

pub async fn route_escalation(
    decider: &Decider,
    persist: &PersistHandle,
    def: &FleetDefinition,
    escalation: &Escalation<'_>,
) -> bool {
    let Some(coordinator) = coordinator_session(def) else {
        return false;
    };
    if coordinator.to_string() == escalation.raised_by {
        return false;
    }
    let asking = Asking {
        source: "problem escalation",
        seat: escalation.raised_by,
        title: escalation.question,
        body: escalation.reason,
    };
    let judged = ask(decider, &asking, DecisionSubject::default()).await;
    let routed = judged.will_route(decider)
        && super::problems::mark_routed(
            persist,
            escalation.run,
            escalation.problem_id,
            escalation.task,
            escalation.question,
        )
        .await
        .is_ok();
    if routed {
        tell_seat(
            persist,
            coordinator,
            &mail_body(&asking, judged.needs.unwrap_or(0.0)),
        )
        .await;
    }
    judged.finish(decider, routed).await;
    routed
}

#[cfg(test)]
#[path = "operator_route_tests.rs"]
mod tests;
