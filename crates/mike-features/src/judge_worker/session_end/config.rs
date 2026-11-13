use serde_json::{Value, json};

use super::EndRule;
use super::rubric::{RUBRIC_ID, system_prompt};

pub const V3_COMPONENTS_VERSION: i64 = 1;
pub const DEFAULT_DIGEST_BUDGET_CHARS: usize = 24_000;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionJudgeV3Config {
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub max_output_tokens: Option<u32>,
    pub rubric_id: &'static str,
    pub slices: Vec<String>,
    pub digest_budget_chars: usize,
    pub default_rules: Vec<EndRule>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BundleComponentError {
    #[error("bundle components are missing `{0}`")]
    Missing(&'static str),
    #[error("rubric `{0}` is not a session-judge v3 rubric")]
    UnknownRubric(String),
    #[error("default rule `{0}` is unknown")]
    UnknownRule(String),
    #[error("slice `{0}` is not supported by the v3 session judge")]
    UnsupportedSlice(String),
}

fn rules_from(c: &Value) -> Result<Vec<EndRule>, BundleComponentError> {
    c.get("default_rules")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|r| {
                    EndRule::parse(r).ok_or_else(|| BundleComponentError::UnknownRule(r.to_owned()))
                })
                .collect()
        })
        .unwrap_or(Ok(Vec::new()))
}

impl SessionJudgeV3Config {
    pub fn to_bundle_components(&self) -> Value {
        json!({
            "components_version": V3_COMPONENTS_VERSION,
            "role": "session_judge",
            "rubric_id": self.rubric_id,
            "system_prompt": system_prompt(self.rubric_id),
            "model": self.model,
            "reasoning_effort": self.reasoning_effort,
            "max_output_tokens": self.max_output_tokens,
            "slices": self.slices,
            "digest_budget_chars": self.digest_budget_chars,
            "default_rules": self.default_rules.iter().map(|r| r.code()).collect::<Vec<_>>(),
        })
    }

    pub fn from_bundle_components(c: &Value) -> Result<Self, BundleComponentError> {
        let rubric = c
            .get("rubric_id")
            .and_then(Value::as_str)
            .ok_or(BundleComponentError::Missing("rubric_id"))?;
        if rubric != RUBRIC_ID {
            return Err(BundleComponentError::UnknownRubric(rubric.to_owned()));
        }
        let slices: Vec<String> = c
            .get("slices")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(s) = slices.first() {
            return Err(BundleComponentError::UnsupportedSlice(s.clone()));
        }
        Ok(Self {
            model: c
                .get("model")
                .and_then(Value::as_str)
                .ok_or(BundleComponentError::Missing("model"))?
                .to_owned(),
            reasoning_effort: c
                .get("reasoning_effort")
                .and_then(Value::as_str)
                .map(str::to_owned),
            max_output_tokens: c
                .get("max_output_tokens")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            rubric_id: RUBRIC_ID,
            slices,
            digest_budget_chars: c
                .get("digest_budget_chars")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(DEFAULT_DIGEST_BUDGET_CHARS),
            default_rules: rules_from(c)?,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::{BundleComponentError, SessionJudgeV3Config};
    use crate::judge_worker::session_end::EndRule;
    use crate::judge_worker::session_end::rubric::RUBRIC_ID;

    fn cfg() -> SessionJudgeV3Config {
        SessionJudgeV3Config {
            model: "google/gemini-3.5-flash".to_owned(),
            reasoning_effort: Some("low".to_owned()),
            max_output_tokens: Some(2_000),
            rubric_id: RUBRIC_ID,
            slices: Vec::new(),
            digest_budget_chars: 24_000,
            default_rules: vec![EndRule::FleetSeatV1, EndRule::DoneV1],
        }
    }

    #[test]
    fn components_round_trip() -> anyhow::Result<()> {
        assert_eq!(
            SessionJudgeV3Config::from_bundle_components(&cfg().to_bundle_components())?,
            cfg()
        );
        Ok(())
    }

    #[test]
    fn an_unknown_rule_or_slice_is_refused() {
        let mut c = cfg().to_bundle_components();
        c["default_rules"] = serde_json::json!(["later-v2"]);
        assert_eq!(
            SessionJudgeV3Config::from_bundle_components(&c),
            Err(BundleComponentError::UnknownRule("later-v2".to_owned()))
        );
        let mut c = cfg().to_bundle_components();
        c["slices"] = serde_json::json!(["related"]);
        assert!(matches!(
            SessionJudgeV3Config::from_bundle_components(&c),
            Err(BundleComponentError::UnsupportedSlice(_))
        ));
    }
}
