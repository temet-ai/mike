use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::placement::{self, Home, Placement, PlacementAnswer};
use super::prompt::write_guidance;
use crate::lesson_shape::assess;
use crate::llm::{CallUsage, LlmError, OpenRouterClient};
use crate::store::persistence::lessons::write_time::{NeighbourLesson, PlannedRelation};
use crate::store::persistence::project_knowledge::GuidanceRow;

pub use crate::settings::memory_prompts::LESSON_RELATE_PROMPT as RELATE_PROMPT;
pub const NEIGHBOUR_LIMIT: i64 = 5;
pub const NEIGHBOUR_MAX_DISTANCE: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Store,
    Skip,
    Update,
    Merge,
    Contradicts,
}

impl Relation {
    pub fn code(self) -> &'static str {
        match self {
            Relation::Store => "store",
            Relation::Skip => "skip",
            Relation::Update => "update",
            Relation::Merge => "merge",
            Relation::Contradicts => "contradicts",
        }
    }

    fn relation_kind(self) -> Option<&'static str> {
        match self {
            Relation::Store => None,
            Relation::Skip => Some("recurs"),
            Relation::Update => Some("supersedes"),
            Relation::Merge => Some("refines"),
            Relation::Contradicts => Some("contradicts"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CandidateDecision {
    pub lesson_id: i64,
    pub decision: Relation,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RelateAnswer {
    #[serde(default)]
    pub decisions: Vec<CandidateDecision>,
    #[serde(default)]
    pub replacement: String,
    #[serde(default)]
    pub placement: Option<PlacementAnswer>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WriteAction {
    Store,
    Skip { target: i64 },
    Replace { replacement: String },
    Discard { covered_by: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteTimeVerdict {
    pub action: WriteAction,
    pub relations: Vec<PlannedRelation>,
    pub placement: Placement,
    pub notes: Vec<String>,
}

impl WriteTimeVerdict {
    pub fn store() -> Self {
        Self {
            action: WriteAction::Store,
            relations: Vec::new(),
            placement: Placement::memory("stored without a placement decision"),
            notes: Vec::new(),
        }
    }

    pub fn replacement(&self) -> Option<&str> {
        match &self.action {
            WriteAction::Replace { replacement } => Some(replacement),
            _ => None,
        }
    }
}

pub struct NewLessonContext {
    pub verdict_id: i64,
    pub source_turn_id: i64,
    pub max_chars: usize,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "decisions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "lesson_id": {"type": "integer"},
                        "decision": {"type": "string", "enum": ["store", "skip", "update", "merge", "contradicts"]},
                        "reason": {"type": "string"},
                    },
                    "required": ["lesson_id", "decision", "reason"],
                },
            },
            "replacement": {"type": "string"},
            "placement": placement::schema(),
        },
        "required": ["decisions", "replacement", "placement"],
    })
}

pub fn build_input(
    rule: &str,
    neighbours: &[NeighbourLesson],
    guidance: &[GuidanceRow],
    max_chars: usize,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## CHARACTER LIMIT FOR replacement\n{max_chars}\n");
    let _ = writeln!(out, "## NEW LESSON\n{rule}\n");
    out.push_str("## EXISTING LESSONS\n");
    if neighbours.is_empty() {
        out.push_str("(none close enough to compare)\n\n");
    }
    for n in neighbours {
        let _ = writeln!(
            out,
            "- lesson_id {} (source {}, recorded {}, similarity {:.2})\n{}\n",
            n.id,
            n.source,
            n.created_at.format("%Y-%m-%d"),
            1.0 - n.distance,
            n.text
        );
    }
    if !guidance.is_empty() {
        write_guidance(&mut out, guidance);
    }
    out
}

pub async fn call(
    llm: &OpenRouterClient,
    system: &str,
    rule: &str,
    neighbours: &[NeighbourLesson],
    guidance: &[GuidanceRow],
    max_chars: usize,
) -> Result<(RelateAnswer, CallUsage), LlmError> {
    llm.verdict_with_usage::<RelateAnswer>(
        system,
        &build_input(rule, neighbours, guidance, max_chars),
        schema(),
    )
    .await
}

fn planned(
    relation: Relation,
    neighbour: &NeighbourLesson,
    reason: &str,
    context: &NewLessonContext,
) -> Option<PlannedRelation> {
    let kind = relation.relation_kind()?;
    Some(PlannedRelation {
        to_lesson_id: neighbour.id,
        kind: kind.to_owned(),
        distance: f64::from(neighbour.distance),
        rationale: reason.to_owned(),
        evidence: json!({
            "decision": relation.code(),
            "verdict_id": context.verdict_id,
            "new_source_turn_id": context.source_turn_id,
            "first_seen_at": neighbour.created_at,
            "related": {
                "lesson_id": neighbour.id,
                "source": neighbour.source,
                "created_at": neighbour.created_at,
                "source_turn_id": neighbour.source_turn_id,
                "session_id": neighbour.session_id,
            },
        }),
    })
}

struct Chosen<'a> {
    neighbour: &'a NeighbourLesson,
    relation: Relation,
    reason: &'a str,
}

fn choose<'a>(
    answer: &'a RelateAnswer,
    neighbours: &'a [NeighbourLesson],
    notes: &mut Vec<String>,
) -> Vec<Chosen<'a>> {
    let mut chosen: Vec<Chosen<'a>> = Vec::new();
    for decision in &answer.decisions {
        let Some(neighbour) = neighbours.iter().find(|n| n.id == decision.lesson_id) else {
            notes.push(format!(
                "decision for lesson {} ignored: it was not one of the lessons shown",
                decision.lesson_id
            ));
            continue;
        };
        if chosen.iter().any(|c| c.neighbour.id == neighbour.id) {
            notes.push(format!(
                "second decision for lesson {} ignored",
                neighbour.id
            ));
            continue;
        }
        chosen.push(Chosen {
            neighbour,
            relation: decision.decision,
            reason: &decision.reason,
        });
    }
    chosen
}

fn usable_replacement(
    answer: &RelateAnswer,
    max_chars: usize,
    notes: &mut Vec<String>,
) -> Option<String> {
    let text = answer.replacement.trim();
    if text.is_empty() {
        notes.push("update or merge chosen without a replacement text".to_owned());
        return None;
    }
    let found = assess(text, None, max_chars);
    if !found.is_rule() {
        notes.push(format!(
            "replacement text rejected, it is not a rule: {}",
            found.detail
        ));
        return None;
    }
    Some(text.to_owned())
}

fn demote_nowhere(placement: &mut Placement, notes: &mut Vec<String>) {
    if placement.home == Home::Nowhere {
        placement.home = Home::Memory;
        notes
            .push("placed nowhere, but the lesson is stored, so it is placed as memory".to_owned());
    }
}

fn final_action(
    replacement: Option<String>,
    placement: &mut Placement,
    contradicts: bool,
    notes: &mut Vec<String>,
) -> WriteAction {
    if let Some(replacement) = replacement {
        demote_nowhere(placement, notes);
        return WriteAction::Replace { replacement };
    }
    match placement.covered_by.clone() {
        Some(covered_by) if placement.home == Home::Nowhere && !contradicts => {
            WriteAction::Discard { covered_by }
        }
        _ => {
            demote_nowhere(placement, notes);
            WriteAction::Store
        }
    }
}

pub fn resolve(
    answer: &RelateAnswer,
    neighbours: &[NeighbourLesson],
    shown: &[GuidanceRow],
    context: &NewLessonContext,
) -> WriteTimeVerdict {
    let mut notes = Vec::new();
    let chosen = choose(answer, neighbours, &mut notes);
    let mut placement = placement::resolve(answer.placement.as_ref(), shown, &mut notes);
    if let Some(skip) = chosen.iter().find(|c| c.relation == Relation::Skip) {
        let relations = planned(Relation::Skip, skip.neighbour, skip.reason, context)
            .into_iter()
            .collect();
        return WriteTimeVerdict {
            action: WriteAction::Skip {
                target: skip.neighbour.id,
            },
            relations,
            placement,
            notes,
        };
    }
    let replaces = chosen
        .iter()
        .any(|c| matches!(c.relation, Relation::Update | Relation::Merge));
    let replacement = replaces
        .then(|| usable_replacement(answer, context.max_chars, &mut notes))
        .flatten();
    let relations = chosen
        .iter()
        .filter(|c| match c.relation {
            Relation::Update | Relation::Merge => replacement.is_some(),
            Relation::Contradicts => true,
            Relation::Store | Relation::Skip => false,
        })
        .filter_map(|c| planned(c.relation, c.neighbour, c.reason, context))
        .collect::<Vec<_>>();
    let contradicts = relations.iter().any(|r| r.kind == "contradicts");
    let action = final_action(replacement, &mut placement, contradicts, &mut notes);
    WriteTimeVerdict {
        action,
        relations,
        placement,
        notes,
    }
}

#[cfg(test)]
#[path = "relate_tests.rs"]
mod tests;
