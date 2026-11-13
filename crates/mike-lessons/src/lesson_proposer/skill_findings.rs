use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerFinding {
    pub query: String,
    pub loaded: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Findings {
    #[serde(default)]
    pub missed_triggers: Vec<TriggerFinding>,
    #[serde(default)]
    pub wrong_triggers: Vec<TriggerFinding>,
}

fn loaded_names(loaded: &[String]) -> String {
    if loaded.is_empty() {
        "no skill".to_owned()
    } else {
        loaded.join(", ")
    }
}

impl Findings {
    pub fn is_empty(&self) -> bool {
        self.missed_triggers.is_empty() && self.wrong_triggers.is_empty()
    }

    pub fn describe(&self) -> String {
        let mut out = String::new();
        if !self.missed_triggers.is_empty() {
            out.push_str("Messages that should have loaded the skill and did not:\n");
            for miss in &self.missed_triggers {
                let _ = writeln!(
                    out,
                    "- \"{}\" (the model loaded: {})",
                    miss.query.trim(),
                    loaded_names(&miss.loaded)
                );
            }
        }
        if !self.wrong_triggers.is_empty() {
            out.push_str("Messages that loaded the skill and should not have:\n");
            for wrong in &self.wrong_triggers {
                let _ = writeln!(out, "- \"{}\"", wrong.query.trim());
            }
        }
        out
    }
}

pub fn describe_eval(eval: &Value) -> String {
    let mut out = String::new();
    if let Some(error) = eval.get("error").and_then(Value::as_str) {
        let _ = writeln!(out, "The evaluation could not run: {error}");
    }
    if let Some(reasons) = eval.pointer("/verdict/reasons").and_then(Value::as_array) {
        for reason in reasons.iter().filter_map(Value::as_str) {
            let _ = writeln!(out, "Rejected because {reason}.");
        }
    }
    if let Some(reason) = eval.pointer("/review/reason").and_then(Value::as_str) {
        let decision = eval
            .pointer("/review/decision")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let _ = writeln!(
            out,
            "The reviewer's decision was {decision}: {}",
            reason.trim()
        );
    }
    if let Some(overlap) = eval.pointer("/review/overlap").and_then(Value::as_str) {
        let _ = writeln!(out, "The reviewer named this existing item: {overlap}.");
    }
    if let Some(place) = eval.pointer("/review/belongs_in").and_then(Value::as_str) {
        let _ = writeln!(out, "The reviewer said the content belongs in: {place}.");
    }
    if let Some(patched) = eval
        .pointer("/review/patched_skill_md")
        .and_then(Value::as_str)
    {
        let _ = write!(
            out,
            "The reviewer's corrected SKILL.md, which also failed:\n{}\n",
            patched.trim()
        );
    }
    let findings: Findings = eval
        .get("findings")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    out.push_str(&findings.describe());
    out
}
