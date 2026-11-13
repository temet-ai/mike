use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CallUsage;
use super::LlmError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecisionRequest {
    pub model: String,
    pub state: Value,
    pub questions: BTreeMap<String, Question>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoulCriteria {
    #[serde(rename = "true")]
    pub yes: String,
    #[serde(rename = "false")]
    pub no: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Question {
    Noul {
        instructions: String,
        criteria: NoulCriteria,
    },
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Noul {
        noul: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Choice {
        choice: String,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Score {
        score: f64,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Refusal,
}

impl Answer {
    pub fn noul(&self) -> Option<f64> {
        match self {
            Self::Noul { noul, .. } => Some(*noul),
            _ => None,
        }
    }

    pub fn is_refusal(&self) -> bool {
        matches!(self, Self::Refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireUsage {
    #[serde(default)]
    pub input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub usage: WireUsage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionResponse {
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub usage: CallUsage,
}

#[derive(Debug)]
pub struct DecisionOutcome {
    pub record_id: Option<i64>,
    pub result: Result<DecisionResponse, LlmError>,
}

impl DecisionResponse {
    pub fn refusals(&self) -> usize {
        self.answers.values().filter(|a| a.is_refusal()).count()
    }

    pub fn noul(&self, question: &str) -> Option<f64> {
        self.answers.get(question).and_then(Answer::noul)
    }
}

fn schema_error(message: String) -> LlmError {
    LlmError::Schema(<serde_json::Error as serde::de::Error>::custom(message))
}

pub fn parse_response(body: &str, asked: &DecisionRequest) -> Result<WireResponse, LlmError> {
    let wire: WireResponse = serde_json::from_str(body)?;
    if let Some(missing) = asked
        .questions
        .keys()
        .find(|q| !wire.answers.contains_key(*q))
    {
        return Err(schema_error(format!(
            "decisions response has no answer for question `{missing}`"
        )));
    }
    if let Some((name, _)) = wire.answers.iter().find(|(_, a)| match a {
        Answer::Noul { noul, .. } => !(0.0..=1.0).contains(noul),
        _ => false,
    }) {
        return Err(schema_error(format!(
            "decisions answer `{name}` is outside 0..1"
        )));
    }
    Ok(wire)
}

pub fn decision_client(
    base: &super::OpenRouterClient,
    overlay: &crate::settings::SettingsOverlay,
) -> Result<super::OpenRouterClient, LlmError> {
    let timeout_ms = u64::try_from(overlay.decisions_timeout_ms.clamp(200, 30_000)).unwrap_or(3000);
    Ok(base
        .with_model(overlay.decisions_model.clone())?
        .with_role(super::LlmRole::Decision)
        .with_timeout(std::time::Duration::from_millis(timeout_ms)))
}

pub fn decisions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    match trimmed.strip_suffix("/v1") {
        Some(api) => format!("{api}/alpha/decisions"),
        None => format!("{trimmed}/alpha/decisions"),
    }
}

#[derive(Clone, Copy)]
pub struct DecisionStore<'a>(pub &'a crate::store::persistence::PersistHandle);

pub fn store(persist: &crate::store::persistence::PersistHandle) -> DecisionStore<'_> {
    DecisionStore(persist)
}

pub mod export;
pub mod loop_store;
pub mod record;

#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod tests;
