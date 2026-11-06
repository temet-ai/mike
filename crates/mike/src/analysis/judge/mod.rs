use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::llm::LlmError;
use crate::store::persistence::operator_turns::JudgeCandidateRow;

pub(crate) mod input_v2;
#[cfg(test)]
mod input_v2_tests;
#[cfg(test)]
mod message_tests;
mod outcome;
pub(crate) mod retry;
#[cfg(test)]
mod retry_tests;
pub(crate) mod rubric;
#[cfg(test)]
mod rubric_tests;
pub(crate) mod run_v2;
#[cfg(test)]
mod schema_tests;
pub(crate) mod two_axis;
#[cfg(test)]
mod two_axis_tests;

pub(crate) use outcome::JudgeOutcome;

pub const JUDGE_RUBRIC_VERSION: &str = rubric::R3_ID;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum JudgeLabel {
    Accepted,
    Shipped,
    Corrected,
    Abandoned,
    Unknown,
}

impl JudgeLabel {
    pub(crate) fn outcome_code(&self) -> &'static str {
        match self {
            JudgeLabel::Accepted => "accepted",
            JudgeLabel::Shipped => "shipped",
            JudgeLabel::Corrected => "corrected",
            JudgeLabel::Abandoned => "abandoned",
            JudgeLabel::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EvidenceQuality {
    Verified,
    Asserted,
    Absent,
}

impl EvidenceQuality {
    pub(crate) fn as_db_code(self) -> &'static str {
        match self {
            EvidenceQuality::Verified => "verified",
            EvidenceQuality::Asserted => "asserted",
            EvidenceQuality::Absent => "absent",
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct JudgeVerdict {
    pub(crate) label: JudgeLabel,
    pub(crate) confidence: f32,
    pub(crate) rationale: String,
    pub(crate) unmet_ask: String,
    pub(crate) evidence_quality: EvidenceQuality,
}

pub(crate) fn judge_verdict_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "label": {
                "type": "string",
                "enum": ["accepted", "shipped", "corrected", "abandoned", "unknown"],
                "description": "The outcome label for this operator turn"
            },
            "confidence": {
                "type": "number",
                "description": "Classifier confidence 0.0-1.0"
            },
            "rationale": {
                "type": "string",
                "description": "One or two sentences citing what in the turn supports the label"
            },
            "unmet_ask": {
                "type": "string",
                "description": "What the operator asked for that this turn did not deliver; empty when nothing was unmet"
            },
            "evidence_quality": {
                "type": "string",
                "enum": ["verified", "asserted", "absent"],
                "description": "How the turn supports its own claims: verified, asserted, or absent"
            }
        },
        "required": ["label", "confidence", "rationale", "unmet_ask", "evidence_quality"],
        "additionalProperties": false
    })
}

pub(crate) fn judge_system_prompt() -> &'static str {
    rubric::R3_PROMPT
}

pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

pub(crate) fn truncate_chars_tail(s: &str, max_chars: usize) -> &str {
    let total = s.chars().count();
    if total <= max_chars {
        return s;
    }
    match s.char_indices().nth(total - max_chars) {
        Some((byte_idx, _)) => &s[byte_idx..],
        None => s,
    }
}

pub(crate) fn build_judge_user_message(c: &JudgeCandidateRow) -> String {
    const MAX_PROMPT_CHARS: usize = 1_000;
    const MAX_ASSISTANT_CHARS: usize = 3_000;

    let prompt = truncate_chars(&c.user_prompt, MAX_PROMPT_CHARS);
    let assistant = truncate_chars_tail(&c.final_assistant_text, MAX_ASSISTANT_CHARS);

    format!(
        "OPERATOR REQUEST (user prompt):\n\
         {prompt}\n\
         \n\
         ASSISTANT TURN TEXT (whole turn, in order; cut at the front when long):\n\
         {assistant}\n\
         \n\
         TOOL USE SUMMARY:\n\
         tool_call_count = {tool_calls}\n\
         tool_error_count = {tool_errors}",
        tool_calls = c.tool_call_count,
        tool_errors = c.tool_error_count,
    )
}

const MAX_TRANSIENT_ATTEMPTS: u32 = 4;

const RETRY_BACKOFF_BASE: Duration = Duration::from_millis(200);

const JITTER_CAP: Duration = Duration::from_millis(150);

fn is_transient(e: &LlmError) -> bool {
    matches!(e, LlmError::Timeout)
        || matches!(e, LlmError::Http { status } if *status == 429 || (500..600).contains(status))
}

fn backoff_for_attempt(attempt: u32, backoff_base: Duration) -> Duration {
    let multiplier = 1u32 << attempt.saturating_sub(1);
    backoff_base * multiplier + jitter()
}

fn jitter() -> Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    JITTER_CAP * (nanos % 1_000) / 1_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JudgeFailure {
    TransientExhausted,
    Permanent,
}

impl JudgeFailure {
    pub(crate) fn as_db_code(self) -> &'static str {
        match self {
            JudgeFailure::TransientExhausted => "transient_exhausted",
            JudgeFailure::Permanent => "permanent",
        }
    }
}

#[cfg(test)]
pub(crate) async fn run_judge(
    llm: &crate::llm::OpenRouterClient,
    c: &JudgeCandidateRow,
    system_prompt: &str,
) -> Result<JudgeVerdict, JudgeOutcome> {
    run_judge_with_backoff_base(llm, c, system_prompt, RETRY_BACKOFF_BASE).await
}

pub(crate) async fn run_judge_with_usage(
    llm: &crate::llm::OpenRouterClient,
    system_prompt: &str,
    user_msg: &str,
    turn_id: i64,
) -> Result<(JudgeVerdict, crate::llm::CallUsage), JudgeOutcome> {
    retry::with_judge_retry(turn_id, RETRY_BACKOFF_BASE, || {
        llm.verdict_with_usage::<JudgeVerdict>(system_prompt, user_msg, judge_verdict_schema())
    })
    .await
}

#[cfg(test)]
async fn run_judge_with_backoff_base(
    llm: &crate::llm::OpenRouterClient,
    c: &JudgeCandidateRow,
    system_prompt: &str,
    backoff_base: Duration,
) -> Result<JudgeVerdict, JudgeOutcome> {
    let user_msg = build_judge_user_message(c);
    retry::with_judge_retry(c.turn_id, backoff_base, || {
        llm.verdict::<JudgeVerdict>(system_prompt, &user_msg, judge_verdict_schema())
    })
    .await
}
