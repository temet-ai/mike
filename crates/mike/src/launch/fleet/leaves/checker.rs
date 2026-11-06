use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{CHECKER_PROMPT, LeafError, LeafModel, parse, require_text};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckerQuestion {
    pub question: String,
    pub artifact: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckerAnswer {
    pub observed: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Checked {
    pub question: String,
    pub observed: bool,
    pub evidence: String,
}

pub(super) fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["observed", "evidence"],
        "properties": {
            "observed": {"type": "boolean"},
            "evidence": {"type": "string"}
        }
    })
}

fn user_text(question: &CheckerQuestion) -> String {
    json!({
        "question": question.question,
        "artifact": question.artifact,
        "expected_observation": question.expected,
    })
    .to_string()
}

pub async fn check(
    model: &dyn LeafModel,
    question: &CheckerQuestion,
) -> Result<Checked, LeafError> {
    require_text("question", &question.question)?;
    require_text("artifact", &question.artifact)?;
    require_text("expected", &question.expected)?;
    let raw = model
        .answer(CHECKER_PROMPT, &user_text(question), schema())
        .await?;
    let answer: CheckerAnswer = parse(raw)?;
    require_text("evidence", &answer.evidence)?;
    Ok(Checked {
        question: question.question.clone(),
        observed: answer.observed,
        evidence: answer.evidence,
    })
}
