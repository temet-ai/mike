use super::input_v2::JudgeInput;
use super::outcome::JudgeOutcome;
use super::retry::with_judge_retry;
use super::rubric::{EffectiveRubric, RubricFamily};
use super::two_axis::{TwoAxisVerdict, two_axis_verdict_schema};
use super::{JudgeFailure, RETRY_BACKOFF_BASE};
use crate::llm::{CallUsage, OpenRouterClient};

pub async fn run_judge_v2(
    llm: &OpenRouterClient,
    rubric: &EffectiveRubric,
    input: &JudgeInput,
    turn_id: i64,
) -> Result<(TwoAxisVerdict, CallUsage), JudgeOutcome> {
    if rubric.base.family != RubricFamily::TwoAxis {
        tracing::warn!(rubric = %rubric.id, "judge v2: refusing a rubric that is not two-axis");
        return Err(JudgeOutcome::Failed(JudgeFailure::Permanent));
    }
    with_judge_retry(turn_id, RETRY_BACKOFF_BASE, || {
        llm.verdict_with_usage::<TwoAxisVerdict>(
            &rubric.system_prompt,
            &input.text,
            two_axis_verdict_schema(),
        )
    })
    .await
}
