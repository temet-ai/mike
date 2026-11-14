use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::pass::{Solver, solve_with};
use super::{Dossier, LeafError, LeafModel, brief, parse, require_text};
use crate::launch::fleet::problems::{Decision, DecisionKind};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::tail_chars;

pub const POINT: &str = "problem_solver";
pub const QUESTION_SET: &str = "problem_solver_v1";
const ACTION: &str = "action";
const DETAIL_CHARS: usize = 1500;
const EVENT_CHARS: usize = 300;
const EVENTS_PER_TASK: usize = 6;

const MEANINGS: [(DecisionKind, &str); 11] = [
    (
        DecisionKind::RecordFault,
        "An environment or mechanism fault to record in the run, with no change to the fleet.",
    ),
    (
        DecisionKind::InstructSeats,
        "An instruction to mail to seats, with no change to the fleet definition.",
    ),
    (
        DecisionKind::PatchDefinition,
        "A patch to a brief, a task's files or dependencies, a rule, a shared file or an operation.",
    ),
    (
        DecisionKind::NewTask,
        "A one-file fix to add as a new task; a closed task is never reopened.",
    ),
    (
        DecisionKind::WaiveObligation,
        "An obligation to waive, with its scope stated.",
    ),
    (
        DecisionKind::AmendRuling,
        "A done_when or ruling command that is wrong and should be corrected.",
    ),
    (
        DecisionKind::AcceptSubstituteProof,
        "A check that cannot run in the run, to accept on named in-run proofs and check live after the deploy.",
    ),
    (
        DecisionKind::DisregardReceipt,
        "A stuck or superseded receipt to drop.",
    ),
    (
        DecisionKind::RerunTargeted,
        "One ruling that names exactly the affected tests, to run again.",
    ),
    (
        DecisionKind::ChangeSetting,
        "A runtime or service setting to change.",
    ),
    (
        DecisionKind::Escalate,
        "Only the operator can settle it; the operator is asked one specific question.",
    ),
];

#[derive(Debug, Deserialize)]
struct QuestionText {
    question: String,
}

pub fn questions() -> BTreeMap<String, Question> {
    let options: Vec<(&str, &str)> = MEANINGS
        .iter()
        .map(|(kind, meaning)| (kind.as_str(), *meaning))
        .collect();
    BTreeMap::from([(
        ACTION.to_owned(),
        choice(
            "A fleet run has a problem. Given the trigger, its detail, the earlier decisions and the recent events of the tasks it concerns, which single action should the problem-solver take?",
            &options,
        ),
    )])
}

pub fn state(dossier: &Dossier) -> Value {
    let tasks: Vec<Value> = dossier
        .tasks
        .iter()
        .map(|trail| {
            let from = trail.recent.len().saturating_sub(EVENTS_PER_TASK);
            let recent: Vec<&str> = trail.recent[from..]
                .iter()
                .map(|line| tail_chars(line, EVENT_CHARS))
                .collect();
            json!({"task": trail.task, "recent_events": recent})
        })
        .collect();
    json!({
        "trigger": dossier.trigger,
        "detail": tail_chars(&dossier.detail, DETAIL_CHARS),
        "earlier_decisions": dossier.earlier_decisions,
        "decisions_before_the_operator": dossier.decisions_before_the_operator,
        "tasks": tasks,
    })
}

pub fn compose(asked: &Asked) -> Option<(DecisionKind, f64)> {
    if asked.failed() {
        return None;
    }
    let (picked, probability) = asked.choice(ACTION)?;
    DecisionKind::parse(picked).map(|kind| (kind, probability))
}

pub fn picked_decision(dossier: &Dossier, kind: DecisionKind, probability: f64) -> Decision {
    let tasks: Vec<&str> = dossier.tasks.iter().map(|t| t.task.as_str()).collect();
    Decision {
        kind,
        decided_by: "problem_solver".to_owned(),
        reason: format!(
            "Jev picked {} from the dossier, {:.0}% sure; trigger {}.",
            kind.as_str(),
            probability * 100.0,
            dossier.trigger
        ),
        evidence_read: vec![format!(
            "the dossier of problem {}: its trigger, detail, earlier decisions and the recent events of {}",
            dossier.problem_id,
            if tasks.is_empty() {
                "the run".to_owned()
            } else {
                tasks.join(", ")
            }
        )],
        question: None,
    }
}

fn question_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["question"],
        "properties": {"question": {"type": "string"}}
    })
}

async fn write_question(model: &dyn LeafModel, dossier: &Dossier) -> Result<String, LeafError> {
    let text = format!(
        "{}\n\nThe decision is already made: escalate to the operator. Write only the one specific question for the operator: what, why only they can answer, and what each answer would set in motion.",
        brief(dossier).text
    );
    let raw = model
        .answer(
            "You are the problem-solver of a fleet run.",
            &text,
            question_schema(),
        )
        .await?;
    let answer: QuestionText = parse(raw)?;
    require_text("question", &answer.question)?;
    Ok(answer.question)
}

pub struct JevSolver<M: LeafModel> {
    pub model: M,
    pub decider: Decider,
}

impl<M: LeafModel> JevSolver<M> {
    async fn resolve(
        &self,
        dossier: &Dossier,
        kind: DecisionKind,
        probability: f64,
    ) -> Option<Decision> {
        let mut decision = picked_decision(dossier, kind, probability);
        if kind == DecisionKind::Escalate {
            decision.question = Some(write_question(&self.model, dossier).await.ok()?);
        }
        Some(decision)
    }
}

#[async_trait]
impl<M: LeafModel> Solver for JevSolver<M> {
    async fn solve(&self, dossier: &Dossier) -> Result<Decision, LeafError> {
        let asked = self
            .decider
            .ask(
                QUESTION_SET,
                state(dossier),
                questions(),
                DecisionSubject::default(),
                json!({}),
            )
            .await;
        let composed = compose(&asked);
        let score = composed.map_or(1.0, |(_, p)| p);
        let decision = match composed.filter(|_| self.decider.applies()) {
            Some((kind, p)) => self.resolve(dossier, kind, p).await,
            None => None,
        };
        let applied = if decision.is_some() { "chosen" } else { "none" };
        let kind = composed.map(|(kind, _)| kind.as_str());
        self.decider
            .mark(
                &asked,
                asked.composed(
                    json!({"score": score, "problem_id": dossier.problem_id, "decision": kind}),
                ),
                applied,
            )
            .await;
        match decision {
            Some(decision) => Ok(decision),
            None => solve_with(&self.model, dossier).await,
        }
    }
}

#[cfg(test)]
#[path = "solver_jev_tests.rs"]
mod tests;
