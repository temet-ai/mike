use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::EvidenceQuality;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reaction {
    Corrected,
    Redirected,
    Accepted,
    NoResponse,
    Unclear,
}

impl Reaction {
    pub(crate) fn code(self) -> &'static str {
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
pub(crate) enum Delivery {
    Shipped,
    Delivered,
    Partial,
    None,
    Question,
    NotApplicable,
}

impl Delivery {
    pub(crate) fn code(self) -> &'static str {
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
pub(crate) enum Severity {
    None,
    Minor,
    Major,
    Severe,
}

impl Severity {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Severity::None => "none",
            Severity::Minor => "minor",
            Severity::Major => "major",
            Severity::Severe => "severe",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TwoAxisVerdict {
    pub(crate) reaction: Reaction,
    pub(crate) delivery: Delivery,
    pub(crate) confidence: f32,
    pub(crate) rationale: String,
    pub(crate) unmet_ask: String,
    pub(crate) evidence_quality: EvidenceQuality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) failure_tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) severity: Option<Severity>,
}

pub(crate) fn two_axis_verdict_schema() -> Value {
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
