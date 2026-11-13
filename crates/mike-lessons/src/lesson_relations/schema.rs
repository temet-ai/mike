use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct ProposerVerdict {
    pub is_related: bool,
    pub kind: String,
    pub from_lesson_id: i64,
    pub to_lesson_id: i64,
    pub rationale: String,
    pub quoted_evidence: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AdjudicatorVerdict {
    pub from_lesson_id: i64,
    pub to_lesson_id: i64,
    pub rationale: String,
}

pub fn proposer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "is_related": {
                "type": "boolean",
                "description": "Whether lesson A and lesson B are genuinely related: one updates, refines, recurs against, or contradicts the other."
            },
            "kind": {
                "type": "string",
                "enum": ["supersedes", "refines", "recurs", "contradicts"],
                "description": "Relation kind. Ignored by the caller when is_related is false."
            },
            "from_lesson_id": {
                "type": "integer",
                "description": "The NEWER lesson's id — the one doing the superseding/refining. Must equal lesson A's or B's id from the prompt."
            },
            "to_lesson_id": {
                "type": "integer",
                "description": "The OLDER lesson's id being superseded/refined. Must equal the other of lesson A's or B's id."
            },
            "rationale": {
                "type": "string",
                "description": "1-3 sentence rationale citing specific evidence from the texts."
            },
            "quoted_evidence": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Exact substrings copied verbatim from lesson A's or B's text that support the verdict. Every string must be an exact quote — never paraphrase or summarize."
            }
        },
        "required": ["is_related", "kind", "from_lesson_id", "to_lesson_id", "rationale", "quoted_evidence"],
        "additionalProperties": false
    })
}

pub fn adjudicator_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "from_lesson_id": {
                "type": "integer",
                "description": "The id of the lesson that YOU judge to be the superseding/updating one, reading the evidence from scratch. Must be one of the two lesson ids shown."
            },
            "to_lesson_id": {
                "type": "integer",
                "description": "The id of the lesson that YOU judge to be superseded/updated. Must be the other of the two lesson ids shown."
            },
            "rationale": {
                "type": "string",
                "description": "1-3 sentence rationale citing specific evidence from the texts."
            }
        },
        "required": ["from_lesson_id", "to_lesson_id", "rationale"],
        "additionalProperties": false
    })
}

use crate::lessons_settings::meta_memory_loop::{
    LESSON_RELATIONS_ADJUDICATOR_PROMPT_KEY, LESSON_RELATIONS_PROPOSER_PROMPT_KEY,
};
use crate::settings::memory_prompts::text_from_rows;
pub use crate::settings::memory_prompts::{
    LESSON_RELATIONS_ADJUDICATOR_PROMPT as ADJUDICATOR_SYSTEM_PROMPT,
    LESSON_RELATIONS_PROPOSER_PROMPT as PROPOSER_SYSTEM_PROMPT,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationPrompts {
    pub proposer: String,
    pub adjudicator: String,
}

impl RelationPrompts {
    pub fn from_rows(rows: &[(String, Value)]) -> Self {
        Self {
            proposer: text_from_rows(
                rows,
                LESSON_RELATIONS_PROPOSER_PROMPT_KEY,
                PROPOSER_SYSTEM_PROMPT,
            ),
            adjudicator: text_from_rows(
                rows,
                LESSON_RELATIONS_ADJUDICATOR_PROMPT_KEY,
                ADJUDICATOR_SYSTEM_PROMPT,
            ),
        }
    }
}
