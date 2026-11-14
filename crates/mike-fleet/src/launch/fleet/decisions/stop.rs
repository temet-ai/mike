use std::collections::BTreeMap;
use std::sync::LazyLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::config::{Mode, Point};
use super::stop_facts::StopFacts;
use super::{FleetDecisions, text};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, noul, record_outcome};
use crate::llm::decisions::record::DecisionSubject;

pub const QUESTION_SET: &str = "fleet_stop_check_v1";
pub const CLAIM_ABOVE: f64 = 0.6;
pub const WAITING_ABOVE: f64 = 0.6;
pub const UNANSWERED_ABOVE: f64 = 0.6;
const MESSAGE_TAIL_BYTES: usize = 1500;

const CLAIMS_DONE: &str = "final_message_claims_done";
const WAITING: &str = "waiting_on_something_running";
const UNANSWERED: &str = "question_to_coordinator_unanswered";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StopRequest {
    #[serde(default)]
    pub final_message: Option<String>,
    #[serde(default)]
    pub stop_hook_active: bool,
    #[serde(default)]
    pub probe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Off,
    LetThrough,
    PushBack,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StopAnswer {
    pub decision: Decision,
    pub reason: String,
    pub facts: Option<StopFacts>,
}

impl StopAnswer {
    pub fn active() -> Self {
        Self {
            decision: Decision::LetThrough,
            reason: "the stop check is active for this seat".to_owned(),
            facts: None,
        }
    }

    pub fn off(reason: &str) -> Self {
        Self {
            decision: Decision::Off,
            reason: reason.to_owned(),
            facts: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Settled(Decision, String),
    Ask,
}

pub fn code_step(facts: &StopFacts, request: &StopRequest) -> Step {
    if facts.check_passed_after_change {
        return Step::Settled(
            Decision::LetThrough,
            "a check passed after the last change".to_owned(),
        );
    }
    if facts.unchecked_changes() {
        if facts.check_running {
            return Step::Settled(
                Decision::LetThrough,
                "a check is running and will report the result".to_owned(),
            );
        }
        if request.stop_hook_active {
            return Step::Settled(
                Decision::LetThrough,
                "the seat was already sent back once".to_owned(),
            );
        }
        return Step::Settled(
            Decision::PushBack,
            format!(
                "{} changed file(s) are uncommitted and no check has passed since; run fleet_check, then fleet_commit",
                facts.uncommitted.len()
            ),
        );
    }
    Step::Ask
}

pub fn questions(with_message: bool) -> BTreeMap<String, Question> {
    let mut asked = BTreeMap::from([
        (
            WAITING.to_owned(),
            noul(
                "Is the seat waiting on something that is running or on someone else, such as a check, a build permit or a reply?",
                "The seat is waiting for something outside its control to finish or arrive.",
                "The seat is not waiting for anything; it stopped on its own.",
            ),
        ),
        (
            UNANSWERED.to_owned(),
            noul(
                "Did the seat ask the coordinator a question that is still unanswered?",
                "A question the seat asked is open and the seat cannot continue until it is answered.",
                "There is no open question the seat is blocked on.",
            ),
        ),
    ]);
    if with_message {
        asked.insert(
            CLAIMS_DONE.to_owned(),
            noul(
                "Does the final message say that the task is finished?",
                "The final message states that the work for the task is complete.",
                "The final message does not claim the task is complete.",
            ),
        );
    }
    asked
}

pub fn compose(asked: &Asked, request: &StopRequest) -> (Decision, String, f64) {
    if asked.failed() {
        return (Decision::Off, "Jev did not answer".to_owned(), 1.0);
    }
    let claims = asked.noul(CLAIMS_DONE).unwrap_or(0.0);
    let waiting = asked.noul(WAITING).unwrap_or(0.0);
    let unanswered = asked.noul(UNANSWERED).unwrap_or(0.0);
    if claims > CLAIM_ABOVE {
        return (
            Decision::LetThrough,
            "the final message says the task is done".to_owned(),
            claims,
        );
    }
    if waiting > WAITING_ABOVE {
        return (
            Decision::LetThrough,
            "the seat is waiting on something outside its control".to_owned(),
            waiting,
        );
    }
    if unanswered > UNANSWERED_ABOVE {
        return (
            Decision::LetThrough,
            "the seat is blocked on an unanswered question".to_owned(),
            unanswered,
        );
    }
    if request.stop_hook_active {
        return (
            Decision::LetThrough,
            "the seat was already sent back once".to_owned(),
            0.5,
        );
    }
    let score = 1.0 - claims.max(waiting).max(unanswered);
    (
        Decision::PushBack,
        "the seat stopped without saying it is done, waiting or blocked".to_owned(),
        score,
    )
}

static PUSHED: LazyLock<DashMap<Uuid, (i64, usize)>> = LazyLock::new(DashMap::new);

async fn label_earlier_pushback(decider: &Decider, session: Uuid, facts: &StopFacts) {
    let Some((_, (record, uncommitted_then))) = PUSHED.remove(&session) else {
        return;
    };
    let progressed = facts.check_passed_after_change || facts.uncommitted.len() < uncommitted_then;
    let label = if progressed { "right" } else { "wrong" };
    record_outcome(
        decider.persist(),
        record,
        label,
        "next_stop_facts",
        json!({
            "uncommitted_then": uncommitted_then,
            "uncommitted_now": facts.uncommitted.len(),
            "check_passed_after_change": facts.check_passed_after_change,
        }),
    )
    .await;
}

pub async fn judge(
    decider: &Decider,
    (facts, request): (&StopFacts, &StopRequest),
    (session, session_db_id, fleet_id): (Uuid, Option<i64>, i64),
) -> (Decision, String) {
    let message = text::tail(
        request.final_message.as_deref().unwrap_or(""),
        MESSAGE_TAIL_BYTES,
    );
    let state: Value = facts.state(message);
    let subject = DecisionSubject {
        session_id: session_db_id,
        fleet_run_id: Some(fleet_id),
        seat: Some(facts.seat.clone()),
        ..DecisionSubject::default()
    };
    let thresholds = json!({
        "claim_above": CLAIM_ABOVE,
        "waiting_above": WAITING_ABOVE,
        "unanswered_above": UNANSWERED_ABOVE,
    });
    let asked = decider
        .ask(
            QUESTION_SET,
            state,
            questions(!message.is_empty()),
            subject,
            thresholds,
        )
        .await;
    let (decision, reason, score) = compose(&asked, request);
    if !decider.applies() {
        decider
            .mark(&asked, asked.composed(json!({"score": score})), "none")
            .await;
        return (Decision::Off, "the stop check is in shadow mode".to_owned());
    }
    let applied = match decision {
        Decision::PushBack => "pushed_back",
        Decision::LetThrough => "let_through",
        Decision::Off => "none",
    };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    if decision == Decision::PushBack
        && let Some(record) = asked.record_id
    {
        PUSHED.insert(session, (record, facts.uncommitted.len()));
    }
    (decision, reason)
}

pub async fn answer(
    decisions: Option<&FleetDecisions>,
    mode: Mode,
    (facts, request): (StopFacts, StopRequest),
    (session, fleet_id): (Uuid, i64),
) -> StopAnswer {
    let Some(decider) = decisions.and_then(|d| d.decider(Point::StopCheck, mode)) else {
        PUSHED.remove(&session);
        return StopAnswer::off("the stop check is off for this seat");
    };
    label_earlier_pushback(&decider, session, &facts).await;
    let (decision, reason) = match code_step(&facts, &request) {
        Step::Settled(decision, reason) if decider.applies() => (decision, reason),
        Step::Settled(..) => (Decision::Off, "the stop check is in shadow mode".to_owned()),
        Step::Ask => {
            let db_id = decider
                .persist()
                .resolve_session_db_id(&session.to_string())
                .await
                .ok()
                .flatten();
            judge(&decider, (&facts, &request), (session, db_id, fleet_id)).await
        }
    };
    StopAnswer {
        decision,
        reason,
        facts: Some(facts),
    }
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stop_db_tests.rs"]
mod db_tests;
