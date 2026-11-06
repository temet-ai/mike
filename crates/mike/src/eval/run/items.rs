use serde_json::Value;

use crate::analysis::judge::input_v2::INPUT_BUILDER_R3;
use crate::analysis::judge::rubric::EffectiveRubric;
use crate::analysis::judge::{JudgeOutcome, build_judge_user_message, run_judge_with_usage};
use crate::judge_worker::v2_judge::{
    V2InsertTarget, V2JudgeConfig, context_from_row, judge_turn_v2, verdict_usage,
};
use crate::llm::{CallUsage, LlmCallContext, LlmRuntimeSettings, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_runs::LabelSetItem;
use crate::store::persistence::operator_turns::JudgeCandidateRow;
use crate::store::persistence::turn_verdicts::{
    JudgeTurnContextRow, TurnVerdictInsert, VerdictJudgement, VerdictMode, VerdictShape,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RunJudge {
    V2(V2JudgeConfig),
    R3 {
        rubric: EffectiveRubric,
        model: String,
        llm_settings: LlmRuntimeSettings,
    },
}

impl RunJudge {
    pub(crate) fn rubric_id(&self) -> &str {
        match self {
            Self::V2(cfg) => &cfg.rubric.id,
            Self::R3 { rubric, .. } => &rubric.id,
        }
    }

    pub(crate) fn builder(&self) -> &'static str {
        match self {
            Self::V2(cfg) => cfg.builder,
            Self::R3 { .. } => INPUT_BUILDER_R3,
        }
    }

    pub(crate) fn model(&self) -> &str {
        match self {
            Self::V2(cfg) => &cfg.model,
            Self::R3 { model, .. } => model,
        }
    }

    pub(crate) fn is_r3(&self) -> bool {
        matches!(self, Self::R3 { .. })
    }

    pub(crate) fn components(&self) -> Value {
        match self {
            Self::V2(cfg) => cfg.to_bundle_components(),
            Self::R3 {
                rubric,
                model,
                llm_settings,
            } => serde_json::json!({
                "components_version": 1,
                "rubric_version": rubric.id,
                "system_prompt": rubric.system_prompt,
                "input_builder": INPUT_BUILDER_R3,
                "model": model,
                "reasoning_effort": llm_settings.reasoning_effort,
                "max_output_tokens": llm_settings.max_output_tokens,
            }),
        }
    }
}

pub(crate) fn judge_from_components(components: &Value) -> Option<RunJudge> {
    let stated = components.get("rubric_version")?.as_str()?;
    let base_id = stated.split('+').next().unwrap_or(stated);
    let base = crate::analysis::judge::rubric::rubric(base_id)?;
    if base.family == crate::analysis::judge::rubric::RubricFamily::TwoAxis {
        return V2JudgeConfig::from_bundle_components(components)
            .ok()
            .map(RunJudge::V2);
    }
    let prompt = components.get("system_prompt")?.as_str()?;
    let defaults = LlmRuntimeSettings::default();
    Some(RunJudge::R3 {
        rubric: crate::analysis::judge::rubric::effective_rubric(base, Some(prompt)),
        model: components.get("model")?.as_str()?.to_owned(),
        llm_settings: LlmRuntimeSettings {
            reasoning_effort: components
                .get("reasoning_effort")
                .and_then(Value::as_str)
                .map_or(defaults.reasoning_effort.clone(), str::to_owned),
            max_output_tokens: components
                .get("max_output_tokens")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(defaults.max_output_tokens),
            ..defaults
        },
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ItemResult {
    Judged {
        verdict_id: i64,
        reaction_correct: bool,
        delivery_correct: Option<bool>,
        usage: CallUsage,
    },
    Refused,
    Failed,
    Skipped,
}

fn failure(outcome: JudgeOutcome) -> ItemResult {
    match outcome {
        JudgeOutcome::Refused => ItemResult::Refused,
        JudgeOutcome::CredentialFailure | JudgeOutcome::Failed(_) => ItemResult::Failed,
    }
}

fn attributed(llm: &OpenRouterClient, row: &JudgeTurnContextRow) -> OpenRouterClient {
    llm.with_call_context(LlmCallContext {
        session_id: Some(row.session_id),
        steerer_id: None,
        operator_turn_id: Some(row.turn_id),
    })
}

pub(crate) struct ItemJob<'a> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) llm: &'a OpenRouterClient,
    pub(crate) judge: &'a RunJudge,
    pub(crate) bundle_id: i64,
    pub(crate) row: &'a JudgeTurnContextRow,
    pub(crate) label: &'a LabelSetItem,
}

async fn judge_v2(job: &ItemJob<'_>, cfg: &V2JudgeConfig) -> anyhow::Result<ItemResult> {
    let Some(ctx) = context_from_row(job.row) else {
        return Ok(ItemResult::Skipped);
    };
    let judged = match judge_turn_v2(&attributed(job.llm, job.row), cfg, &ctx).await {
        Ok(judged) => judged,
        Err(outcome) => return Ok(failure(outcome)),
    };
    let target = V2InsertTarget {
        mode: VerdictMode::Eval,
        bundle_id: Some(job.bundle_id),
    };
    let verdict_id = job
        .persist
        .insert_turn_verdict(judged.to_insert(cfg, target))
        .await?;
    let delivery_correct = job
        .label
        .delivery_label
        .as_deref()
        .map(|gold| gold == judged.verdict.delivery.code());
    Ok(ItemResult::Judged {
        verdict_id,
        reaction_correct: judged.verdict.reaction.code() == job.label.reaction_label,
        delivery_correct,
        usage: judged.usage,
    })
}

async fn judge_r3(
    job: &ItemJob<'_>,
    rubric: &EffectiveRubric,
    model: &str,
    llm_settings: &LlmRuntimeSettings,
) -> anyhow::Result<ItemResult> {
    let candidate = JudgeCandidateRow {
        turn_id: job.row.turn_id,
        user_prompt: job.row.user_prompt.clone(),
        final_assistant_text: job.row.final_assistant_text.clone(),
        tool_call_count: job.row.tool_call_count,
        tool_error_count: job.row.tool_error_count,
    };
    let message = build_judge_user_message(&candidate);
    let llm = attributed(job.llm, job.row)
        .with_model(model.to_owned())?
        .with_runtime_settings(llm_settings.clone());
    let (verdict, usage) =
        match run_judge_with_usage(&llm, &rubric.system_prompt, &message, job.row.turn_id).await {
            Ok(v) => v,
            Err(outcome) => return Ok(failure(outcome)),
        };
    let label = verdict.label.outcome_code();
    let insert = TurnVerdictInsert {
        turn_id: job.row.turn_id,
        mode: VerdictMode::Eval,
        bundle_id: Some(job.bundle_id),
        rubric_version: &rubric.id,
        input_builder_version: INPUT_BUILDER_R3,
        slices: Vec::new(),
        model,
        judgement: VerdictJudgement {
            shape: VerdictShape::Legacy { label },
            confidence: verdict.confidence,
            rationale: &verdict.rationale,
            unmet_ask: &verdict.unmet_ask,
            evidence_quality: verdict.evidence_quality.as_db_code(),
        },
        next_genuine_request_id: None,
        as_of: job.row.closed_at,
        input_text: &message,
        usage: verdict_usage(&usage),
    };
    let verdict_id = job.persist.insert_turn_verdict(insert).await?;
    Ok(ItemResult::Judged {
        verdict_id,
        reaction_correct: (label == "corrected") == (job.label.reaction_label == "corrected"),
        delivery_correct: None,
        usage,
    })
}

pub(crate) async fn judge_item(job: &ItemJob<'_>) -> anyhow::Result<ItemResult> {
    match job.judge {
        RunJudge::V2(cfg) => judge_v2(job, cfg).await,
        RunJudge::R3 {
            rubric,
            model,
            llm_settings,
        } => judge_r3(job, rubric, model, llm_settings).await,
    }
}
