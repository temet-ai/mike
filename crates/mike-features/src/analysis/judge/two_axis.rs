use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::EvidenceQuality;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reaction {
    Corrected,
    Redirected,
    Accepted,
    NoResponse,
    Unclear,
}

impl Reaction {
    pub fn code(self) -> &'static str {
        match self {
            Reaction::Corrected => "corrected",
            Reaction::Redirected => "redirected",
            Reaction::Accepted => "accepted",
            Reaction::NoResponse => "no_response",
            Reaction::Unclear => "unclear",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Shipped,
    Delivered,
    Partial,
    None,
    Question,
    NotApplicable,
}

impl Delivery {
    pub fn code(self) -> &'static str {
        match self {
            Delivery::Shipped => "shipped",
            Delivery::Delivered => "delivered",
            Delivery::Partial => "partial",
            Delivery::None => "none",
            Delivery::Question => "question",
            Delivery::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    None,
    Minor,
    Major,
    Severe,
}

impl Severity {
    pub fn code(self) -> &'static str {
        match self {
            Severity::None => "none",
            Severity::Minor => "minor",
            Severity::Major => "major",
            Severity::Severe => "severe",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoAxisVerdict {
    pub reaction: Reaction,
    pub delivery: Delivery,
    pub confidence: f32,
    pub rationale: String,
    pub unmet_ask: String,
    pub evidence_quality: EvidenceQuality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
}

pub fn two_axis_verdict_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "reaction": {"type": "string", "enum": ["corrected", "redirected", "accepted", "no_response", "unclear"]},
            "delivery": {"type": "string", "enum": ["shipped", "delivered", "partial", "none", "question", "not_applicable"]},
            "confidence": {"type": "number", "description": "Confidence 0.0-1.0 for the reaction axis"},
            "rationale": {"type": "string"},
            "unmet_ask": {"type": "string"},
            "evidence_quality": {"type": "string", "enum": ["verified", "asserted", "absent"]}
        },
        "required": ["reaction", "delivery", "confidence", "rationale", "unmet_ask", "evidence_quality"],
        "additionalProperties": false
    })
}
