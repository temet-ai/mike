use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub(crate) struct ProposerVerdict {
    pub(super) is_related: bool,
    pub(super) kind: String,
    pub(super) from_lesson_id: i64,
    pub(super) to_lesson_id: i64,
    pub(super) rationale: String,
    pub(super) quoted_evidence: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AdjudicatorVerdict {
    pub(super) from_lesson_id: i64,
    pub(super) to_lesson_id: i64,
    pub(super) rationale: String,
}

pub(super) fn proposer_schema() -> Value {
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

pub(super) fn adjudicator_schema() -> Value {
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

pub(super) const PROPOSER_SYSTEM_PROMPT: &str = "\
You are proposing a directional relationship between two stored lessons from an AI \
coding agent's memory. A relation means one lesson (the NEWER one) updates, refines, \
recurs against, or contradicts the other (the OLDER one) — never that they are merely \
similar in topic. Only claim is_related=true when the text gives a concrete reason: an \
explicit resolution, correction, retraction, or restatement of the other lesson's claim. \
Every string in quoted_evidence MUST be copied character-for-character from lesson A's \
or B's text — never paraphrase, summarize, or invent a quote.";

pub(super) const ADJUDICATOR_SYSTEM_PROMPT: &str = "\
You are independently reviewing another model's proposed relationship between two \
stored lessons. You receive the SAME evidence the proposer saw, plus the proposer's \
claim. Form your own judgment from the evidence — do not defer to the proposer's \
rationale. Disagree when the evidence does not support the claimed kind or direction. \
The claim names each lesson twice, once as LESSON A / LESSON B and once by id; both \
refer to the same lesson. Do not answer whether you 'agree' — instead state the \
direction YOU read out of the evidence by returning from_lesson_id (the superseding \
lesson) and to_lesson_id (the superseded one). Agreement is computed by comparing your \
direction to the proposer's, so say what you actually believe and let the comparison \
fall where it falls.";
