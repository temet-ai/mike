use serde_json::{Value, json};

use crate::analysis::judge::input_v2::{
    JudgeInput, JudgeTurnContext, build_judge_input_v2, builder_for,
};
use crate::analysis::judge::rubric::{EffectiveRubric, RubricFamily, effective_rubric, rubric};
use crate::analysis::judge::run_v2::run_judge_v2;
use crate::analysis::judge::two_axis::{TwoAxisVerdict, two_axis_verdict_schema};
use crate::analysis::judge::{JudgeFailure, JudgeOutcome};
use crate::llm::{CallUsage, LlmRuntimeSettings, OpenRouterClient};
use crate::store::persistence::turn_verdicts::{
    JudgeTurnContextRow, TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode,
    VerdictShape,
};

pub const V2_COMPONENTS_VERSION: i64 = 1;

const NEXT_STATE_PENDING: &str = "pending";
const NEXT_STATE_SETTLED_NONE: &str = "none_session_settled";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum V2ConfigError {
    #[error("bundle components are missing the string key `{0}`")]
    MissingKey(&'static str),
    #[error("bundle components name rubric `{0}`, which is not in the registry")]
    UnknownRubric(String),
    #[error("rubric `{0}` is not a two-axis rubric")]
    NotTwoAxis(String),
    #[error(
        "bundle components state rubric `{stated}` but the stored prompt hashes to `{computed}`"
    )]
    RubricMismatch { stated: String, computed: String },
    #[error("bundle components name input builder `{0}`, which is not a v2 builder")]
    UnknownBuilder(String),
    #[error("bundle components ask for history slices {0:?}; no slice is approved")]
    SlicesNotSupported(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct V2JudgeConfig {
    pub rubric: EffectiveRubric,
    pub builder: &'static str,
    pub model: String,
    pub llm_settings: LlmRuntimeSettings,
}

fn string_key<'a>(c: &'a Value, key: &'static str) -> Result<&'a str, V2ConfigError> {
    c.get(key)
        .and_then(Value::as_str)
        .ok_or(V2ConfigError::MissingKey(key))
}

fn requested_slices(c: &Value) -> Vec<String> {
    c.get("slices")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_rubric(c: &Value) -> Result<EffectiveRubric, V2ConfigError> {
    let stated = string_key(c, "rubric_version")?;
    let base_id = stated.split('+').next().unwrap_or(stated);
    let base = rubric(base_id).ok_or_else(|| V2ConfigError::UnknownRubric(base_id.to_owned()))?;
    if base.family != RubricFamily::TwoAxis {
        return Err(V2ConfigError::NotTwoAxis(base_id.to_owned()));
    }
    let effective = effective_rubric(base, Some(string_key(c, "system_prompt")?));
    if effective.id != stated {
        return Err(V2ConfigError::RubricMismatch {
            stated: stated.to_owned(),
            computed: effective.id,
        });
    }
    Ok(effective)
}

fn llm_settings_from(c: &Value) -> LlmRuntimeSettings {
    let defaults = LlmRuntimeSettings::default();
    LlmRuntimeSettings {
        reasoning_effort: c
            .get("reasoning_effort")
            .and_then(Value::as_str)
            .map_or(defaults.reasoning_effort.clone(), str::to_owned),
        max_output_tokens: c
            .get("max_output_tokens")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(defaults.max_output_tokens),
        ..defaults
    }
}

impl V2JudgeConfig {
    pub fn from_bundle_components(c: &Value) -> Result<Self, V2ConfigError> {
        let slices = requested_slices(c);
        if !slices.is_empty() {
            return Err(V2ConfigError::SlicesNotSupported(slices));
        }
        let rubric = resolve_rubric(c)?;
        let builder_raw = string_key(c, "input_builder")?;
        let builder = builder_for(builder_raw)
            .ok_or_else(|| V2ConfigError::UnknownBuilder(builder_raw.to_owned()))?;
        Ok(Self {
            rubric,
            builder,
            model: string_key(c, "model")?.to_owned(),
            llm_settings: llm_settings_from(c),
        })
    }

    pub fn to_bundle_components(&self) -> Value {
        json!({
            "components_version": V2_COMPONENTS_VERSION,
            "rubric_version": self.rubric.id,
            "system_prompt": self.rubric.system_prompt,
            "output_schema": two_axis_verdict_schema(),
            "input_builder": self.builder,
            "slices": Vec::<String>::new(),
            "model": self.model,
            "reasoning_effort": self.llm_settings.reasoning_effort,
            "max_output_tokens": self.llm_settings.max_output_tokens,
        })
    }
}

#[derive(Debug, Clone)]
pub struct V2Judged {
    pub verdict: TwoAxisVerdict,
    pub usage: CallUsage,
    pub input: JudgeInput,
    pub model: Option<String>,
    pub jev: Option<crate::analysis::judge::jev::JevVerdict>,
}

pub struct V2InsertTarget {
    pub mode: VerdictMode,
    pub bundle_id: Option<i64>,
}

impl V2Judged {
    pub fn to_insert<'a>(
        &'a self,
        cfg: &'a V2JudgeConfig,
        target: V2InsertTarget,
    ) -> TurnVerdictInsert<'a> {
        let v = &self.verdict;
        TurnVerdictInsert {
            turn_id: self.input.turn_id,
            mode: target.mode,
            bundle_id: target.bundle_id,
            rubric_version: &cfg.rubric.id,
            input_builder_version: self.input.builder_version,
            slices: Vec::new(),
            model: self.model.as_deref().unwrap_or(&cfg.model),
            judgement: VerdictJudgement {
                shape: VerdictShape::TwoAxis {
                    reaction: v.reaction.code(),
                    delivery: v.delivery.code(),
                    failure_tags: v.failure_tags.clone(),
                    severity: v.severity.map(|s| s.code()),
                },
                confidence: v.confidence,
                rationale: &v.rationale,
                unmet_ask: &v.unmet_ask,
                evidence_quality: v.evidence_quality.as_db_code(),
            },
            next_genuine_request_id: self.input.next_genuine_request_id,
            as_of: self.input.as_of,
            input_text: &self.input.text,
            usage: verdict_usage(&self.usage),
        }
    }
}

pub fn verdict_usage(u: &CallUsage) -> VerdictCallUsage {
    VerdictCallUsage {
        generation_id: u.generation_id.clone(),
        cost_usd: u.cost_usd,
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        latency_ms: u.latency_ms,
    }
}

pub fn context_from_row(row: &JudgeTurnContextRow) -> Option<JudgeTurnContext> {
    if row.next_state == NEXT_STATE_PENDING {
        return None;
    }
    let next_genuine_text = if row.next_state == NEXT_STATE_SETTLED_NONE {
        None
    } else {
        row.next_genuine_text.clone()
    };
    Some(JudgeTurnContext {
        turn_id: row.turn_id,
        session_id: row.session_id,
        user_prompt: row.user_prompt.clone(),
        final_assistant_text: row.final_assistant_text.clone(),
        text_cut_at: row.text_cut_at,
        tool_call_count: row.tool_call_count,
        tool_error_count: row.tool_error_count,
        next_genuine_request_id: row.next_genuine_request_id,
        next_genuine_text,
        next_genuine_at: row.next_genuine_at,
        closed_at: row.closed_at,
    })
}

pub async fn judge_turn_v2(
    llm: &OpenRouterClient,
    cfg: &V2JudgeConfig,
    ctx: &JudgeTurnContext,
) -> Result<V2Judged, JudgeOutcome> {
    let llm = llm
        .with_model(cfg.model.clone())
        .map_err(|_| JudgeOutcome::Failed(JudgeFailure::Permanent))?
        .with_runtime_settings(cfg.llm_settings.clone());
    let input = build_judge_input_v2(ctx, cfg.builder);
    let (verdict, usage) = run_judge_v2(&llm, &cfg.rubric, &input, ctx.turn_id).await?;
    Ok(V2Judged {
        verdict,
        usage,
        input,
        model: None,
        jev: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{V2ConfigError, V2JudgeConfig};
    use crate::analysis::judge::input_v2::INPUT_BUILDER_V2;
    use crate::analysis::judge::rubric::{V2_2026_09_23, effective_rubric};
    use crate::llm::LlmRuntimeSettings;

    fn config() -> V2JudgeConfig {
        V2JudgeConfig {
            rubric: effective_rubric(&V2_2026_09_23, None),
            builder: INPUT_BUILDER_V2,
            model: "google/gemini-3.5-flash".to_owned(),
            llm_settings: LlmRuntimeSettings::default(),
        }
    }

    #[test]
    fn bundle_components_round_trip_to_the_same_config() -> anyhow::Result<()> {
        let cfg = config();
        let back = V2JudgeConfig::from_bundle_components(&cfg.to_bundle_components())?;
        assert_eq!(back, cfg);
        Ok(())
    }

    #[test]
    fn components_with_a_slice_are_refused() {
        let mut c = config().to_bundle_components();
        c["slices"] = serde_json::json!(["turnfacts"]);
        assert_eq!(
            V2JudgeConfig::from_bundle_components(&c),
            Err(V2ConfigError::SlicesNotSupported(vec![
                "turnfacts".to_owned()
            ]))
        );
    }

    #[test]
    fn components_whose_prompt_does_not_match_the_stated_rubric_are_refused() {
        let mut c = config().to_bundle_components();
        c["system_prompt"] = serde_json::json!("a different prompt");
        assert!(matches!(
            V2JudgeConfig::from_bundle_components(&c),
            Err(V2ConfigError::RubricMismatch { .. })
        ));
    }

    #[test]
    fn components_naming_the_r3_rubric_are_refused() {
        let mut c = config().to_bundle_components();
        c["rubric_version"] = serde_json::json!(crate::analysis::judge::rubric::R3_ID);
        assert!(matches!(
            V2JudgeConfig::from_bundle_components(&c),
            Err(V2ConfigError::NotTwoAxis(_))
        ));
    }
}
