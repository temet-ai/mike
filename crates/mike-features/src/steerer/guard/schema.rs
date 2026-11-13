use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::Question;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub applies: bool,
    pub confidence: f64,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GuardVerdict {
    #[serde(default)]
    pub questions: BTreeMap<String, QuestionAnswer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerCheck {
    Applies,
    NoEvidence,
    Declined,
    Missing,
}

impl GuardVerdict {
    pub fn answer(&self, question: Question) -> Option<&QuestionAnswer> {
        self.questions.get(question.code())
    }

    pub fn check(&self, question: Question) -> AnswerCheck {
        match self.answer(question) {
            None => AnswerCheck::Missing,
            Some(a) if !a.applies => AnswerCheck::Declined,
            Some(a) if a.evidence.iter().all(|e| e.trim().is_empty()) => AnswerCheck::NoEvidence,
            Some(_) => AnswerCheck::Applies,
        }
    }
}

fn answer_schema(question: Question) -> Value {
    let mut properties = json!({
        "applies": {"type": "boolean"},
        "confidence": {"type": "number", "minimum": 0, "maximum": 1},
        "evidence": {"type": "array", "items": {"type": "string"}},
        "quote": {"type": "string"},
        "reason": {"type": "string"},
    });
    let mut required = vec!["applies", "confidence", "evidence", "quote", "reason"];
    if question == Question::AuthorisedHandback
        && let Value::Object(map) = &mut properties
    {
        map.insert("class".to_owned(), json!({"type": "string"}));
        required.push("class");
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required,
    })
}

pub fn guard_verdict_schema(open: &[Question]) -> Value {
    let mut props = serde_json::Map::new();
    for q in open {
        props.insert(q.code().to_owned(), answer_schema(*q));
    }
    let required: Vec<&str> = open.iter().map(|q| q.code()).collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "questions": {
                "type": "object",
                "additionalProperties": false,
                "properties": Value::Object(props),
                "required": required,
            }
        },
        "required": ["questions"],
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn verdict(raw: Value) -> GuardVerdict {
        serde_json::from_value(raw).unwrap_or_default()
    }

    #[test]
    fn applies_without_evidence_becomes_no_evidence() {
        let v = verdict(
            json!({"questions": {"relapse": {"applies": true, "confidence": 0.9, "evidence": [], "quote": "q", "reason": "r"}}}),
        );
        assert_eq!(v.check(Question::Relapse), AnswerCheck::NoEvidence);
    }

    #[test]
    fn unknown_question_key_ignored() {
        let v = verdict(
            json!({"questions": {"bogus": {"applies": true, "confidence": 1.0, "evidence": ["req 1"]},
                                             "relapse": {"applies": false, "confidence": 0.2}}}),
        );
        assert_eq!(v.check(Question::Relapse), AnswerCheck::Declined);
        assert_eq!(v.check(Question::UnbackedWait), AnswerCheck::Missing);
    }

    #[test]
    fn missing_open_question_is_llm_call_failed() {
        let v = verdict(json!({"questions": {}}));
        assert_eq!(v.check(Question::UnobservedDone), AnswerCheck::Missing);
        let schema = guard_verdict_schema(&[Question::AuthorisedHandback]);
        assert_eq!(
            schema["properties"]["questions"]["required"][0],
            "authorised_handback"
        );
        assert!(schema["properties"]["questions"]["properties"]["authorised_handback"]["properties"]["class"].is_object());
    }
}
