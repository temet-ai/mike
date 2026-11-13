use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::events::{
    EventPayload, FleetEvent, NewFleetEvent, ProblemDecided, ProblemEscalated, ProblemOpened,
};
use crate::store::persistence::PersistHandle;

pub const TURNS_PER_PROBLEM: u32 = 40;
pub const MINUTES_PER_PROBLEM: i64 = 60;
pub const DECISIONS_PER_PROBLEM: usize = 3;
pub const PROBLEMS_PER_RUN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    RedBaseline,
    UnownedFailure,
    Unestablished,
    WorkerQuestion,
    DisputedFail,
    CriterionFailedThrice,
    StalledSeat,
    CriterionAmended,
    BudgetExhausted,
    AllSeatsIdle,
    EnvironmentFault,
    PredatingGateFailure,
}

impl Trigger {
    pub const ALL: [Trigger; 12] = [
        Trigger::RedBaseline,
        Trigger::UnownedFailure,
        Trigger::Unestablished,
        Trigger::WorkerQuestion,
        Trigger::DisputedFail,
        Trigger::CriterionFailedThrice,
        Trigger::StalledSeat,
        Trigger::CriterionAmended,
        Trigger::BudgetExhausted,
        Trigger::AllSeatsIdle,
        Trigger::EnvironmentFault,
        Trigger::PredatingGateFailure,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::RedBaseline => "red_baseline",
            Trigger::UnownedFailure => "unowned_failure",
            Trigger::Unestablished => "unestablished",
            Trigger::WorkerQuestion => "worker_question",
            Trigger::DisputedFail => "disputed_fail",
            Trigger::CriterionFailedThrice => "criterion_failed_thrice",
            Trigger::StalledSeat => "stalled_seat",
            Trigger::CriterionAmended => "criterion_amended",
            Trigger::BudgetExhausted => "budget_exhausted",
            Trigger::AllSeatsIdle => "all_seats_idle",
            Trigger::EnvironmentFault => "environment_fault",
            Trigger::PredatingGateFailure => "predating_gate_failure",
        }
    }

    pub fn parse(name: &str) -> Option<Trigger> {
        Self::ALL.into_iter().find(|t| t.as_str() == name)
    }

    pub fn needs_opus(self) -> bool {
        matches!(self, Trigger::CriterionFailedThrice | Trigger::DisputedFail)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    RecordFault,
    InstructSeats,
    PatchDefinition,
    NewTask,
    WaiveObligation,
    AmendRuling,
    AcceptSubstituteProof,
    DisregardReceipt,
    RerunTargeted,
    ChangeSetting,
    Escalate,
}

impl DecisionKind {
    pub const ALL: [DecisionKind; 11] = [
        DecisionKind::RecordFault,
        DecisionKind::InstructSeats,
        DecisionKind::PatchDefinition,
        DecisionKind::NewTask,
        DecisionKind::WaiveObligation,
        DecisionKind::AmendRuling,
        DecisionKind::AcceptSubstituteProof,
        DecisionKind::DisregardReceipt,
        DecisionKind::RerunTargeted,
        DecisionKind::ChangeSetting,
        DecisionKind::Escalate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DecisionKind::RecordFault => "record_fault",
            DecisionKind::InstructSeats => "instruct_seats",
            DecisionKind::PatchDefinition => "patch_definition",
            DecisionKind::NewTask => "new_task",
            DecisionKind::WaiveObligation => "waive_obligation",
            DecisionKind::AmendRuling => "amend_ruling",
            DecisionKind::AcceptSubstituteProof => "accept_substitute_proof",
            DecisionKind::DisregardReceipt => "disregard_receipt",
            DecisionKind::RerunTargeted => "rerun_targeted",
            DecisionKind::ChangeSetting => "change_setting",
            DecisionKind::Escalate => "escalate",
        }
    }

    pub fn parse(name: &str) -> Option<DecisionKind> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }

    pub fn names() -> Vec<&'static str> {
        Self::ALL.iter().map(|k| k.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub kind: DecisionKind,
    pub decided_by: String,
    pub reason: String,
    pub evidence_read: Vec<String>,
    pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub id: Uuid,
    pub trigger: String,
    pub task: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub decisions: Vec<String>,
    pub escalated: bool,
}

impl Problem {
    pub fn is_active(&self) -> bool {
        !self.escalated && self.decisions.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Ledger {
    problems: Vec<Problem>,
}

impl Ledger {
    pub fn from_events(events: &[FleetEvent]) -> Ledger {
        let mut ordered: Vec<&FleetEvent> = events.iter().collect();
        ordered.sort_by_key(|e| (e.occurred_at, e.id));
        let mut index = BTreeMap::<Uuid, usize>::new();
        let mut problems = Vec::<Problem>::new();
        for event in ordered {
            match event.decode() {
                Ok(EventPayload::ProblemOpened(opened)) => {
                    index.insert(opened.problem_id, problems.len());
                    problems.push(Problem {
                        id: opened.problem_id,
                        trigger: opened.trigger,
                        task: event.task.clone(),
                        opened_at: event.occurred_at,
                        decisions: Vec::new(),
                        escalated: false,
                    });
                }
                Ok(EventPayload::ProblemDecided(decided)) => {
                    if let Some(at) = index.get(&decided.problem_id) {
                        problems[*at].decisions.push(decided.decision);
                    }
                }
                Ok(EventPayload::ProblemEscalated(escalated)) => {
                    if let Some(at) = index.get(&escalated.problem_id) {
                        problems[*at].escalated = true;
                    }
                }
                _ => {}
            }
        }
        Ledger { problems }
    }

    pub fn opened(&self) -> usize {
        self.problems.len()
    }

    pub fn get(&self, id: Uuid) -> Option<&Problem> {
        self.problems.iter().find(|p| p.id == id)
    }

    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    fn active_like(&self, trigger: Trigger, task: Option<&str>) -> Option<&Problem> {
        self.problems
            .iter()
            .find(|p| p.is_active() && p.trigger == trigger.as_str() && p.task.as_deref() == task)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    pub problem_id: Uuid,
    pub fresh: bool,
    pub events: Vec<NewFleetEvent>,
}

fn event(run: &str, task: Option<&str>, payload: EventPayload) -> NewFleetEvent {
    NewFleetEvent {
        run_id: run.to_string(),
        task: task.map(str::to_string),
        session_uuid: None,
        payload,
    }
}

fn escalation(
    run: &str,
    task: Option<&str>,
    id: Uuid,
    reason: &str,
    question: &str,
) -> NewFleetEvent {
    event(
        run,
        task,
        EventPayload::ProblemEscalated(ProblemEscalated {
            problem_id: id,
            reason: reason.to_string(),
            question: question.to_string(),
        }),
    )
}

pub fn seed_problem(
    run: &str,
    trigger: Trigger,
    task: Option<&str>,
    detail: &str,
) -> (Uuid, NewFleetEvent) {
    let id = Uuid::new_v4();
    let opened = event(
        run,
        task,
        EventPayload::ProblemOpened(ProblemOpened {
            problem_id: id,
            trigger: trigger.as_str().to_string(),
            detail: detail.to_string(),
        }),
    );
    (id, opened)
}

pub fn open_events(
    ledger: &Ledger,
    run: &str,
    trigger: Trigger,
    task: Option<&str>,
    detail: &str,
) -> Opening {
    if let Some(existing) = ledger.active_like(trigger, task) {
        return Opening {
            problem_id: existing.id,
            fresh: false,
            events: Vec::new(),
        };
    }
    let (id, opened) = seed_problem(run, trigger, task, detail);
    let mut events = vec![opened];
    if ledger.opened() >= PROBLEMS_PER_RUN {
        let question = format!(
            "The run has opened {PROBLEMS_PER_RUN} problems; this one ({}) goes to you: {detail}",
            trigger.as_str()
        );
        events.push(escalation(run, task, id, "run_budget", &question));
    }
    Opening {
        problem_id: id,
        fresh: true,
        events,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecideError {
    #[error("no problem {0} in this run; use a problem_id from the dossier")]
    Unknown(Uuid),
    #[error("problem {0} is already escalated to the operator; it takes no further decision")]
    Escalated(Uuid),
    #[error("reason is empty; say what in the evidence led to this decision")]
    MissingReason,
    #[error("evidence_read is empty; list the files, receipts or notes the decision rests on")]
    MissingEvidence,
    #[error("decision escalate needs question: the one specific question for the operator")]
    MissingQuestion,
}

fn check_decision(decision: &Decision) -> Result<(), DecideError> {
    if decision.reason.trim().is_empty() {
        return Err(DecideError::MissingReason);
    }
    if decision.evidence_read.is_empty() {
        return Err(DecideError::MissingEvidence);
    }
    let asked = decision
        .question
        .as_deref()
        .is_some_and(|q| !q.trim().is_empty());
    if decision.kind == DecisionKind::Escalate && !asked {
        return Err(DecideError::MissingQuestion);
    }
    Ok(())
}

fn escalation_reason(decision: &Decision, taken: usize, minutes: i64) -> Option<&'static str> {
    if decision.kind == DecisionKind::Escalate {
        Some("solver_escalated")
    } else if taken + 1 >= DECISIONS_PER_PROBLEM {
        Some("decision_budget")
    } else if minutes > MINUTES_PER_PROBLEM {
        Some("time_budget")
    } else {
        None
    }
}

pub fn decide_events(
    ledger: &Ledger,
    run: &str,
    problem_id: Uuid,
    decision: &Decision,
    now: DateTime<Utc>,
) -> Result<Vec<NewFleetEvent>, DecideError> {
    let problem = ledger
        .get(problem_id)
        .ok_or(DecideError::Unknown(problem_id))?;
    if problem.escalated {
        return Err(DecideError::Escalated(problem_id));
    }
    check_decision(decision)?;
    let minutes = (now - problem.opened_at).num_minutes().max(0);
    let task = problem.task.as_deref();
    let mut events = vec![event(
        run,
        task,
        EventPayload::ProblemDecided(ProblemDecided {
            problem_id,
            decided_by: decision.decided_by.clone(),
            decision: decision.kind.as_str().to_string(),
            reason: decision.reason.clone(),
            evidence_read: decision.evidence_read.clone(),
            minutes,
        }),
    )];
    if let Some(why) = escalation_reason(decision, problem.decisions.len(), minutes) {
        let question = decision.question.clone().unwrap_or_else(|| {
            format!(
                "Problem {} ({}) did not hold after {} decisions: {}",
                problem.trigger,
                problem_id,
                problem.decisions.len() + 1,
                decision.reason
            )
        });
        events.push(escalation(run, task, problem_id, why, &question));
    }
    Ok(events)
}

pub fn overdue_events(ledger: &Ledger, run: &str, now: DateTime<Utc>) -> Vec<NewFleetEvent> {
    let limit = Duration::minutes(MINUTES_PER_PROBLEM);
    ledger
        .problems()
        .iter()
        .filter(|p| p.is_active() && now - p.opened_at > limit)
        .map(|p| {
            let question = format!(
                "Problem {} ({}) had no decision within {MINUTES_PER_PROBLEM} minutes.",
                p.trigger, p.id
            );
            escalation(run, p.task.as_deref(), p.id, "time_budget", &question)
        })
        .collect()
}

async fn append_all(persist: &PersistHandle, events: &[NewFleetEvent]) -> Result<(), String> {
    for new in events {
        persist
            .fleet()
            .append_fleet_event(new)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn ledger(persist: &PersistHandle, run: &str) -> Result<Ledger, String> {
    let events = persist
        .fleet()
        .fleet_events_since(run, 0)
        .await
        .map_err(|err| err.to_string())?;
    Ok(Ledger::from_events(&events))
}

pub async fn open(
    persist: &PersistHandle,
    run: &str,
    trigger: Trigger,
    task: Option<&str>,
    detail: &str,
) -> Result<Opening, String> {
    let current = ledger(persist, run).await?;
    let opening = open_events(&current, run, trigger, task, detail);
    append_all(persist, &opening.events).await?;
    Ok(opening)
}

pub async fn decide(
    persist: &PersistHandle,
    run: &str,
    problem_id: Uuid,
    decision: &Decision,
) -> Result<Vec<NewFleetEvent>, String> {
    let current = ledger(persist, run).await?;
    let events = decide_events(&current, run, problem_id, decision, Utc::now())
        .map_err(|err| err.to_string())?;
    append_all(persist, &events).await?;
    Ok(events)
}

#[cfg(test)]
#[path = "problems_tests.rs"]
mod tests;
