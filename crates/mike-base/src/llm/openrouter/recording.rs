use std::time::Duration;

use serde_json::Value;

use super::call::UsageInfo;
use super::{LlmCallOutcome, LlmCallRecord, LlmError, OpenRouterClient};

pub struct RecordFields<'a> {
    pub usage: Option<&'a UsageInfo>,
    pub generation_id: Option<&'a str>,
    pub http_status: Option<u16>,
    pub latency: Duration,
    pub request_body: Option<&'a Value>,
}

fn record_outcome(e: &LlmError) -> Option<LlmCallOutcome> {
    match e {
        LlmError::Timeout => Some(LlmCallOutcome::Timeout),
        LlmError::Http { .. } => Some(LlmCallOutcome::HttpError),
        LlmError::OutputTruncated { .. } => Some(LlmCallOutcome::Truncated),
        LlmError::Transport(_) => Some(LlmCallOutcome::TransportError),
        LlmError::CeilingExceeded { .. }
        | LlmError::RateLimited { .. }
        | LlmError::SpendCeilingExceeded { .. }
        | LlmError::ProviderUnavailable { .. }
        | LlmError::Schema(_) => None,
    }
}

impl OpenRouterClient {
    pub fn record_if_configured(&self, outcome: LlmCallOutcome, fields: RecordFields<'_>) {
        let Some(sink) = &self.inner.usage_sink else {
            return;
        };
        let should_record = self.effective_settings().is_none_or(|s| s.record_usage);
        if !should_record {
            return;
        }

        sink.record(LlmCallRecord {
            role: self.inner.role,
            model: self.wire_model(),
            generation_id: fields.generation_id.map(str::to_owned),
            prompt_tokens: fields.usage.and_then(|u| u.prompt_tokens),
            completion_tokens: fields.usage.and_then(|u| u.completion_tokens),
            reasoning_tokens: fields.usage.and_then(|u| u.reasoning_tokens),
            cached_tokens: fields.usage.and_then(|u| u.cached_tokens),
            cost_usd: fields.usage.and_then(|u| u.cost_usd),
            latency_ms: i32::try_from(fields.latency.as_millis()).ok(),
            outcome,
            http_status: fields.http_status.map(i32::from),
            session_id: self.inner.call_context.session_id,
            steerer_id: self.inner.call_context.steerer_id,
            operator_turn_id: self.inner.call_context.operator_turn_id,
            request_body: fields.request_body.cloned(),
            response_body: None,
        });
    }

    pub fn record_error_if_known(&self, e: &LlmError, fields: RecordFields<'_>) {
        if let Some(outcome) = record_outcome(e) {
            self.record_if_configured(outcome, fields);
        }
    }
}
