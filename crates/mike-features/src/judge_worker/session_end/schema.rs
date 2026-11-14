use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::judge_worker::session_poll::digest::{JudgeVerdict, judge_verdict_schema};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionVerdictV3 {
    pub outcome: String,
    pub quality_score: f64,
    pub rationale: String,
    pub overview: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub learnings: Vec<String>,
    pub end_label: String,
    pub end_evidence: String,
    pub end_confidence: String,
    pub failure: String,
    #[serde(default)]
    pub failure_tags: Vec<String>,
    pub severity: String,
}

impl SessionVerdictV3 {
    pub fn legacy(&self) -> JudgeVerdict {
        JudgeVerdict {
            outcome: self.outcome.clone(),
            quality_score: self.quality_score,
            quality_unknown: false,
            rationale: self.rationale.clone(),
            overview: self.overview.clone(),
            topics: self.topics.clone(),
            learnings: self.learnings.clone(),
        }
    }
}

pub fn session_verdict_v3_schema() -> Value {
    let mut schema = judge_verdict_schema();
    let extra = json!({
        "end_label": {
            "type": "string",
            "enum": ["done", "closed_in_frustration", "cut_off_neutral", "fleet_seat", "unclear"],
            "description": "Why the session ended, per the session-end rubric."
        },
        "end_evidence": {"type": "string", "description": "The deciding words, quoted verbatim, or an empty string."},
        "end_confidence": {"type": "string", "enum": ["high", "medium", "low"]},
        "failure": {"type": "string", "description": "For closed_in_frustration: one sentence on what the assistant did; otherwise empty."},
        "failure_tags": {"type": "array", "items": {"type": "string"}, "maxItems": 3},
        "severity": {"type": "string", "enum": ["none", "minor", "major", "severe"]}
    });
    if let (Some(props), Some(extra)) = (
        schema.get_mut("properties").and_then(Value::as_object_mut),
        extra.as_object(),
    ) {
        for (k, v) in extra {
            props.insert(k.clone(), v.clone());
        }
    }
    let required: Vec<&str> = vec![
        "outcome",
        "quality_score",
        "rationale",
        "overview",
        "topics",
        "learnings",
        "end_label",
        "end_evidence",
        "end_confidence",
        "failure",
        "failure_tags",
        "severity",
    ];
    schema["required"] = json!(required);
    schema["additionalProperties"] = json!(false);
    schema
}

#[cfg(test)]
mod tests {
    use super::session_verdict_v3_schema;

    #[test]
    fn the_v3_schema_requires_the_six_end_fields() {
        let s = session_verdict_v3_schema();
        let required: Vec<&str> = s["required"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        for f in [
            "end_label",
            "end_evidence",
            "end_confidence",
            "failure",
            "failure_tags",
            "severity",
            "overview",
        ] {
            assert!(required.contains(&f), "{f}");
            assert!(s["properties"].get(f).is_some(), "{f}");
        }
        assert_eq!(s["additionalProperties"], false);
    }
}
