use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::prompt::write_guidance;
use crate::llm::{CallUsage, LlmError, OpenRouterClient};
use crate::store::persistence::project_knowledge::GuidanceRow;

pub const PLACE_PROMPT: &str = concat!(
    include_str!("prompt/place.md"),
    "\n\n",
    include_str!("prompt/homes.md")
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Home {
    Rule,
    Skill,
    Agent,
    Memory,
    Nowhere,
}

impl Home {
    pub fn code(self) -> &'static str {
        match self {
            Home::Rule => "rule",
            Home::Skill => "skill",
            Home::Agent => "agent",
            Home::Memory => "memory",
            Home::Nowhere => "nowhere",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlacementAnswer {
    pub home: Home,
    #[serde(default)]
    pub covered_by: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub persona: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub home: Home,
    pub covered_by: Option<String>,
    pub reason: String,
}

impl Placement {
    pub fn memory(reason: &str) -> Self {
        Self {
            home: Home::Memory,
            covered_by: None,
            reason: reason.to_owned(),
        }
    }

    pub fn is_covered(&self) -> bool {
        self.home == Home::Nowhere && self.covered_by.is_some()
    }
}

pub struct LessonToPlace<'a> {
    pub text: &'a str,
    pub kind: &'a str,
    pub topic: Option<&'a str>,
    pub agent_name: Option<&'a str>,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "home": {"type": "string", "enum": ["rule", "skill", "agent", "memory", "nowhere"]},
            "covered_by": {"type": "string"},
            "reason": {"type": "string"},
            "persona": {"type": "boolean"},
        },
        "required": ["home", "covered_by", "reason", "persona"],
    })
}

fn shown_document<'a>(named: &str, shown: &'a [GuidanceRow]) -> Option<&'a GuidanceRow> {
    let named = named.trim();
    if named.is_empty() {
        return None;
    }
    shown.iter().find(|row| row.cited_by(named))
}

pub fn resolve(
    answer: Option<&PlacementAnswer>,
    shown: &[GuidanceRow],
    notes: &mut Vec<String>,
) -> Placement {
    let Some(answer) = answer else {
        notes.push("no placement in the answer; placed as memory".to_owned());
        return Placement::memory("the model gave no placement");
    };
    let covered_by = match shown_document(&answer.covered_by, shown) {
        Some(row) => Some(row.relative_path.clone()),
        None => {
            if !answer.covered_by.trim().is_empty() {
                notes.push(format!(
                    "covered_by {:?} ignored: it is not one of the documents shown",
                    answer.covered_by.trim()
                ));
            }
            None
        }
    };
    let reason = match answer.reason.trim() {
        "" => "the model gave no reason".to_owned(),
        text => text.to_owned(),
    };
    let home = match answer.home {
        Home::Agent if !answer.persona => {
            notes.push(
                "placed as agent but the lesson is not persona; placed as memory instead"
                    .to_owned(),
            );
            Home::Memory
        }
        Home::Nowhere if covered_by.is_none() => {
            notes.push(
                "placed nowhere without a shown document covering it; placed as memory instead"
                    .to_owned(),
            );
            Home::Memory
        }
        home => home,
    };
    Placement {
        home,
        covered_by,
        reason,
    }
}

pub fn build_input(lesson: &LessonToPlace<'_>, guidance: &[GuidanceRow]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## LESSON\n{}\n", lesson.text.trim());
    let _ = writeln!(out, "## KIND\n{}\n", lesson.kind);
    if let Some(topic) = lesson.topic {
        let _ = writeln!(out, "## TOPIC\n{topic}\n");
    }
    if let Some(agent) = lesson.agent_name {
        let _ = writeln!(out, "## SCOPED TO AGENT\n{agent}\n");
    }
    if !guidance.is_empty() {
        write_guidance(&mut out, guidance);
    }
    out
}

pub async fn call(
    llm: &OpenRouterClient,
    lesson: &LessonToPlace<'_>,
    guidance: &[GuidanceRow],
) -> Result<(PlacementAnswer, CallUsage), LlmError> {
    llm.verdict_with_usage::<PlacementAnswer>(
        PLACE_PROMPT,
        &build_input(lesson, guidance),
        schema(),
    )
    .await
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
