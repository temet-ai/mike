mod call;
pub(crate) mod demotion;

use chrono::{Duration, Utc};
use serde::Serialize;
use serde_json::json;

use std::sync::Arc;

use crate::llm::{LlmCallContext, LlmRuntimeSettings, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_view::OpenTextRow;
use crate::store::persistence::steering::{RegradeOlderVersions, ReplyLabelCandidate};
use crate::store::persistence::steering_evaluations::EvaluationLabel;
use crate::store::persistence::tool_call_reads::PairedToolCallRow;
use call::{ReplyRequest, ReplyVerdict, call_reply_labeller, label_for};

pub(crate) const REPLY_LABELLER_VERSION: &str = "reply-labeller-v2";
pub(crate) const REPLY_SOURCE: &str = "session_reply";
const PER_PASS: i64 = 20;
const WINDOW_DAYS: i64 = 14;
const REGRADE_WINDOW_HOURS: i64 = 72;
const REPLY_SETTLES_AFTER_SECS: f64 = 600.0;
const FAILURES_BEFORE_STOP: usize = 3;
const REASONING_EFFORT: &str = "medium";
const MAX_OUTPUT_TOKENS: u32 = 4_000;

fn labeller_client(llm: &OpenRouterClient) -> OpenRouterClient {
    llm.with_live_settings(Arc::new(|| LlmRuntimeSettings {
        reasoning_effort: REASONING_EFFORT.to_owned(),
        max_output_tokens: MAX_OUTPUT_TOKENS,
        ..LlmRuntimeSettings::default()
    }))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub(crate) struct ReplyLabelReport {
    pub(crate) candidates: usize,
    pub(crate) calls: usize,
    pub(crate) failures: usize,
    pub(crate) labelled: u64,
}

struct Turn {
    text: String,
    tools: Vec<PairedToolCallRow>,
}

async fn turn_of(persist: &PersistHandle, request_id: Option<i64>) -> Turn {
    let Some(id) = request_id else {
        return Turn {
            text: String::new(),
            tools: Vec::new(),
        };
    };
    let text = persist
        .fetch_accumulated_text_for_request(id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let tools = persist
        .fetch_request_tool_calls_paired(id)
        .await
        .unwrap_or_default();
    Turn { text, tools }
}

pub(super) fn reply_text_from(rows: &[OpenTextRow], delivered_request_id: i64) -> String {
    rows.iter()
        .filter(|row| row.request_id >= delivered_request_id)
        .map(|row| row.content.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn reply_of(persist: &PersistHandle, c: &ReplyLabelCandidate) -> Turn {
    let mut reply = turn_of(persist, c.delivered_request_id).await;
    let (Some(turn_id), Some(delivered)) = (c.reply_turn_id, c.delivered_request_id) else {
        return reply;
    };
    match persist.fetch_open_turn_text(turn_id, Utc::now()).await {
        Ok(rows) => {
            let whole = reply_text_from(&rows, delivered);
            if !whole.is_empty() {
                reply.text = whole;
            }
        }
        Err(error) => {
            tracing::warn!(evaluation = %c.uuid, %error, "reply labeller: could not read the whole reply turn; grading from its first request");
        }
    }
    reply
}

fn undelivered(c: &ReplyLabelCandidate) -> EvaluationLabel {
    let why = if c.reverted { "reverted" } else { "expired" };
    EvaluationLabel {
        uuid: c.uuid,
        label: "indeterminate".to_owned(),
        source: REPLY_SOURCE.to_owned(),
        evidence: json!({
            "labeller_version": REPLY_LABELLER_VERSION,
            "reaction": "no_reply",
            "undelivered": why,
            "revert_reason": c.revert_reason,
        }),
    }
}

fn from_verdict(c: &ReplyLabelCandidate, v: &ReplyVerdict, model: &str) -> EvaluationLabel {
    EvaluationLabel {
        uuid: c.uuid,
        label: label_for(&v.note_correct).to_owned(),
        source: REPLY_SOURCE.to_owned(),
        evidence: json!({
            "labeller_version": REPLY_LABELLER_VERSION,
            "model": model,
            "note_correct": v.note_correct,
            "reaction": v.reaction,
            "failure": v.failure,
            "quote": v.quote,
            "trigger_request_id": c.trigger_request_id,
            "delivered_request_id": c.delivered_request_id,
        }),
    }
}

async fn judge(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    c: &ReplyLabelCandidate,
) -> Result<EvaluationLabel, String> {
    let turn = turn_of(persist, c.trigger_request_id).await;
    let reply = reply_of(persist, c).await;
    let llm = llm.with_call_context(LlmCallContext {
        session_id: Some(c.session_id),
        steerer_id: Some(c.steerer_id),
        operator_turn_id: Some(c.operator_turn_id),
    });
    let verdict = call_reply_labeller(
        &llm,
        &ReplyRequest {
            steerer: &c.steerer_name,
            rule: c.steerer_rule.as_deref().unwrap_or(""),
            note: c.payload_text.as_deref().unwrap_or(""),
            turn_text: &turn.text,
            turn_tools: &turn.tools,
            reply_text: &reply.text,
            reply_tools: &reply.tools,
        },
    )
    .await?;
    Ok(from_verdict(c, &verdict, llm.model()))
}

async fn label_all(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    candidates: &[ReplyLabelCandidate],
    report: &mut ReplyLabelReport,
) -> Vec<EvaluationLabel> {
    let mut labels = Vec::new();
    for c in candidates {
        if c.delivered_request_id.is_none() {
            labels.push(undelivered(c));
            continue;
        }
        if report.failures >= FAILURES_BEFORE_STOP {
            break;
        }
        report.calls += 1;
        match judge(persist, llm, c).await {
            Ok(label) => labels.push(label),
            Err(error) => {
                report.failures += 1;
                tracing::warn!(evaluation = %c.uuid, %error, "reply labeller: call failed");
            }
        }
    }
    labels
}

async fn candidates(persist: &PersistHandle) -> Vec<ReplyLabelCandidate> {
    let since = Utc::now() - Duration::days(WINDOW_DAYS);
    let regrade = RegradeOlderVersions {
        current_version: REPLY_LABELLER_VERSION,
        since: Utc::now() - Duration::hours(REGRADE_WINDOW_HOURS),
        reply_settles_after_secs: REPLY_SETTLES_AFTER_SECS,
    };
    match persist
        .reply_label_candidates(PER_PASS, since, regrade)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "reply labeller: candidate read failed");
            Vec::new()
        }
    }
}

async fn write_labels(persist: &PersistHandle, labels: &[EvaluationLabel]) -> u64 {
    match persist.apply_evaluation_labels(labels).await {
        Ok(n) => n,
        Err(error) => {
            tracing::warn!(%error, count = labels.len(), "reply labeller: label write failed");
            0
        }
    }
}

pub(crate) async fn run_reply_label_pass(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
) -> ReplyLabelReport {
    let mut report = ReplyLabelReport::default();
    let candidates = candidates(persist).await;
    report.candidates = candidates.len();
    let llm = labeller_client(llm);
    let labels = label_all(persist, &llm, &candidates, &mut report).await;
    report.labelled = write_labels(persist, &labels).await;
    report
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
