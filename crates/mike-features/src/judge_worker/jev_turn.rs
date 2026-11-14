use crate::analysis::judge::input_v2::JudgeTurnContext;
use crate::analysis::judge::jev::{self, Composed, JevVerdict, Shape, TurnFacts};
use crate::analysis::judge::two_axis::TwoAxisVerdict;
use crate::analysis::judge::{JudgeVerdict, run_judge_with_usage};
use crate::llm::decisions::point::Decider;
use crate::llm::{CallUsage, OpenRouterClient};
use crate::store::persistence::JudgeCandidateRow;

use super::v2_judge::{V2JudgeConfig, V2Judged, judge_turn_v2};

#[cfg(test)]
#[path = "jev_turn_tests.rs"]
mod jev_turn_tests;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Words {
    pub rationale: String,
    pub unmet_ask: String,
}

pub fn two_axis_verdict(composed: &Composed, words: Option<Words>) -> TwoAxisVerdict {
    let words = words.unwrap_or_else(|| Words {
        rationale: composed.summary(),
        unmet_ask: String::new(),
    });
    TwoAxisVerdict {
        reaction: composed.reaction,
        delivery: composed.delivery,
        confidence: composed.confidence as f32,
        rationale: words.rationale,
        unmet_ask: words.unmet_ask,
        evidence_quality: composed.evidence,
        failure_tags: None,
        severity: None,
    }
}

pub fn legacy_verdict(composed: &Composed, words: Option<Words>) -> JudgeVerdict {
    let words = words.unwrap_or_else(|| Words {
        rationale: composed.summary(),
        unmet_ask: String::new(),
    });
    JudgeVerdict {
        label: composed.label.clone(),
        confidence: composed.confidence as f32,
        rationale: words.rationale,
        unmet_ask: words.unmet_ask,
        evidence_quality: composed.evidence,
    }
}

pub fn facts_of(ctx: &JudgeTurnContext) -> TurnFacts<'_> {
    TurnFacts {
        turn_id: ctx.turn_id,
        session_id: Some(ctx.session_id),
        operator_request: &ctx.user_prompt,
        assistant_text: &ctx.final_assistant_text,
        tool_call_count: ctx.tool_call_count,
        tool_error_count: ctx.tool_error_count,
        next_message: ctx.next_genuine_text.as_deref(),
    }
}

pub async fn judge_v2(
    decider: &Decider,
    llm: &OpenRouterClient,
    cfg: &V2JudgeConfig,
    ctx: &JudgeTurnContext,
    threshold: f32,
) -> Option<V2Judged> {
    let shape = Shape::TwoAxis {
        has_next: ctx.next_genuine_text.is_some(),
    };
    let verdict = jev::decide(decider, &facts_of(ctx), shape, threshold).await?;
    let text = if verdict.composed.uncertain() {
        judge_turn_v2(llm, cfg, ctx).await.ok()
    } else {
        None
    };
    let (words, usage) = match text {
        Some(judged) => (
            Some(Words {
                rationale: judged.verdict.rationale,
                unmet_ask: judged.verdict.unmet_ask,
            }),
            judged.usage,
        ),
        None => (None, CallUsage::default()),
    };
    let input = crate::analysis::judge::input_v2::build_judge_input_v2(ctx, cfg.builder);
    Some(V2Judged {
        verdict: two_axis_verdict(&verdict.composed, words),
        usage,
        input,
        model: Some(decider.model().to_owned()),
        jev: Some(verdict),
    })
}

pub struct LegacyJudged {
    pub verdict: JudgeVerdict,
    pub usage: CallUsage,
    pub jev: JevVerdict,
}

pub async fn judge_legacy(
    decider: &Decider,
    llm: &OpenRouterClient,
    c: &JudgeCandidateRow,
    system_prompt: &str,
    user_msg: &str,
    threshold: f32,
) -> Option<LegacyJudged> {
    let facts = TurnFacts {
        turn_id: c.turn_id,
        session_id: None,
        operator_request: &c.user_prompt,
        assistant_text: &c.final_assistant_text,
        tool_call_count: c.tool_call_count,
        tool_error_count: c.tool_error_count,
        next_message: None,
    };
    let verdict = jev::decide(decider, &facts, Shape::Legacy, threshold).await?;
    let text = if verdict.composed.uncertain() {
        run_judge_with_usage(llm, system_prompt, user_msg, c.turn_id)
            .await
            .ok()
    } else {
        None
    };
    let (words, usage) = match text {
        Some((judged, usage)) => (
            Some(Words {
                rationale: judged.rationale,
                unmet_ask: judged.unmet_ask,
            }),
            usage,
        ),
        None => (None, CallUsage::default()),
    };
    Some(LegacyJudged {
        verdict: legacy_verdict(&verdict.composed, words),
        usage,
        jev: verdict,
    })
}
