use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::checker::{Checked, CheckerQuestion, check};
use super::{JUDGE_PROMPT, LeafError, LeafModel, parse, require_text};
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, VerdictRecorded};
use crate::launch::fleet::floor::Outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeVerdict {
    Pass,
    Fail,
    Check,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeAnswer {
    pub verdict: JudgeVerdict,
    pub criteria: Vec<String>,
    pub sha: String,
    pub evidence_read: Vec<String>,
    pub needed: Option<String>,
    pub observation: String,
    #[serde(default)]
    pub checks: Vec<CheckerQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportFacts {
    pub commit: String,
    pub criteria: Vec<String>,
    pub exercised_command: String,
    pub exercised_exit: Option<i32>,
    pub exercised_output_ref: String,
    pub tests_changed: Vec<String>,
    pub unestablished: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CriterionText {
    pub id: String,
    pub text: String,
    pub check: String,
}

#[derive(Debug, Clone)]
pub struct JudgeInput {
    pub task: String,
    pub criteria: Vec<CriterionText>,
    pub report: ReportFacts,
    pub floor: Outcome,
    pub diff: String,
    pub open_obligations: Vec<String>,
    pub task_brief: String,
    pub shared_with: Vec<SharedTask>,
    pub earlier_verdicts: Vec<EarlierVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EarlierVerdict {
    pub round: usize,
    pub passed: bool,
    pub criteria: Vec<String>,
    pub needed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedTask {
    pub task: String,
    pub name: String,
    pub criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    pub answer: JudgeAnswer,
    pub checked: Vec<Checked>,
}

pub(super) fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["verdict", "criteria", "sha", "evidence_read", "needed", "observation", "checks"],
        "properties": {
            "verdict": {"type": "string", "enum": ["pass", "fail", "check"]},
            "criteria": {"type": "array", "items": {"type": "string"}},
            "sha": {"type": "string"},
            "evidence_read": {"type": "array", "items": {"type": "string"}},
            "needed": {"type": ["string", "null"]},
            "observation": {"type": "string"},
            "checks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["question", "artifact", "expected"],
                    "properties": {
                        "question": {"type": "string"},
                        "artifact": {"type": "string"},
                        "expected": {"type": "string"}
                    }
                }
            }
        }
    })
}

fn user_text(input: &JudgeInput, checked: &[Checked]) -> String {
    json!({
        "task": input.task,
        "criteria": input.criteria,
        "report": input.report,
        "floor": {"passed": true, "sha": input.floor.sha()},
        "diff": input.diff,
        "open_obligations": input.open_obligations,
        "task_brief": input.task_brief,
        "shared_with": input.shared_with,
        "round": input.earlier_verdicts.len() + 1,
        "earlier_verdicts": input.earlier_verdicts,
        "checker_results": checked,
        "may_ask_checker": checked.is_empty(),
    })
    .to_string()
}

fn validate(answer: &JudgeAnswer, input: &JudgeInput) -> Result<(), LeafError> {
    if answer.sha != input.floor.sha() {
        return Err(LeafError::Invalid(format!(
            "sha {} is not the reported commit {}",
            answer.sha,
            input.floor.sha()
        )));
    }
    require_text("observation", &answer.observation)?;
    if answer.evidence_read.is_empty() {
        return Err(LeafError::Invalid("evidence_read is empty".into()));
    }
    if answer.verdict == JudgeVerdict::Check {
        return Ok(());
    }
    if answer.criteria.is_empty() {
        return Err(LeafError::Invalid("criteria is empty".into()));
    }
    if let Some(stray) = answer
        .criteria
        .iter()
        .find(|id| !input.criteria.iter().any(|c| &c.id == *id))
    {
        return Err(LeafError::Invalid(format!(
            "criteria names {stray}, which this task does not serve"
        )));
    }
    let needed = answer.needed.as_deref().unwrap_or_default();
    if answer.verdict == JudgeVerdict::Fail {
        require_text("needed", needed)?;
    }
    Ok(())
}

async fn ask(
    model: &dyn LeafModel,
    input: &JudgeInput,
    checked: &[Checked],
) -> Result<JudgeAnswer, LeafError> {
    let raw = model
        .answer(JUDGE_PROMPT, &user_text(input, checked), schema())
        .await?;
    let answer: JudgeAnswer = parse(raw)?;
    validate(&answer, input)?;
    Ok(answer)
}

async fn commissioned(
    model: &dyn LeafModel,
    questions: &[CheckerQuestion],
) -> Result<Vec<Checked>, LeafError> {
    if questions.is_empty() {
        return Err(LeafError::Invalid(
            "verdict check needs checks: the questions for the checker".into(),
        ));
    }
    let mut checked = Vec::new();
    for question in questions {
        checked.push(check(model, question).await?);
    }
    Ok(checked)
}

pub async fn judge(model: &dyn LeafModel, input: &JudgeInput) -> Result<Judged, LeafError> {
    if !input.floor.reaches_judge() {
        return Err(LeafError::FloorNotPassed {
            sha: input.floor.sha().to_string(),
        });
    }
    let first = ask(model, input, &[]).await?;
    if first.verdict != JudgeVerdict::Check {
        return Ok(Judged {
            answer: first,
            checked: Vec::new(),
        });
    }
    let checked = commissioned(model, &first.checks).await?;
    let second = ask(model, input, &checked).await?;
    if second.verdict == JudgeVerdict::Check {
        return Err(LeafError::Invalid(
            "verdict check is allowed once; decide pass or fail from the checker results".into(),
        ));
    }
    Ok(Judged {
        answer: second,
        checked,
    })
}

pub fn verdict_events(
    run: &str,
    task: &str,
    seat: Option<Uuid>,
    note_uuid: Uuid,
    answer: &JudgeAnswer,
) -> Vec<NewFleetEvent> {
    let passed = answer.verdict == JudgeVerdict::Pass;
    answer
        .criteria
        .iter()
        .map(|criterion| NewFleetEvent {
            run_id: run.to_string(),
            task: Some(task.to_string()),
            session_uuid: seat,
            payload: EventPayload::VerdictRecorded(VerdictRecorded {
                note_uuid,
                criterion: criterion.clone(),
                passed,
                needed: answer.needed.clone().unwrap_or_default(),
            }),
        })
        .collect()
}
