use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use wiremock::MockServer;

use super::{CallLimiter, LlmCallRecord, LlmUsageSink, OpenRouterClient};
use crate::llm::usage::RoleSpend;

#[derive(Debug, Deserialize, PartialEq)]
pub struct TestVerdict {
    pub label: bool,
}

pub fn test_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "label": {"type": "boolean"}
        },
        "required": ["label"],
        "additionalProperties": false
    })
}

pub fn valid_response(label: bool) -> Value {
    json!({
        "choices": [{
            "message": {
                "content": format!(r#"{{"label":{label}}}"#)
            }
        }]
    })
}

pub fn empty_content_response() -> Value {
    json!({
        "choices": [{
            "message": {
                "content": ""
            }
        }]
    })
}

pub fn missing_content_response() -> Value {
    json!({
        "choices": [{
            "message": {}
        }]
    })
}

pub fn response_with_usage(label: bool, finish_reason: Option<&str>) -> Value {
    let mut choice = json!({
        "message": { "content": format!(r#"{{"label":{label}}}"#) }
    });
    if let Some(fr) = finish_reason {
        choice["finish_reason"] = json!(fr);
    }
    json!({
        "id": "gen-abc123",
        "choices": [choice],
        "usage": {
            "prompt_tokens": 120,
            "completion_tokens": 40,
            "cost": 0.0123,
            "completion_tokens_details": {"reasoning_tokens": 30},
            "prompt_tokens_details": {"cached_tokens": 10}
        }
    })
}

/// A client with its own limiter: the process-wide one is shared by every test
/// in the binary, so tests that each make a few calls would exhaust its
/// per-minute budget for one another.
pub fn client_for(server: &MockServer) -> anyhow::Result<OpenRouterClient> {
    client_with_limiter(server, fresh_limiter())
}

pub fn fresh_limiter() -> CallLimiter {
    CallLimiter::with_limits(crate::llm::limiter::LimiterLimits::compiled_defaults())
}

pub fn client_with_limiter(
    server: &MockServer,
    limiter: CallLimiter,
) -> anyhow::Result<OpenRouterClient> {
    OpenRouterClient::new_with_limiter(
        "sk-test".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        Some(server.uri()),
        None,
        Arc::new(limiter),
    )
    .map_err(Into::into)
}

#[derive(Default)]
pub struct TestSink {
    pub calls: std::sync::Mutex<Vec<LlmCallRecord>>,
    spend: RoleSpend,
}

impl TestSink {
    pub fn with_spend(role: crate::llm::LlmRole, usd: f64) -> Self {
        let mut spend = RoleSpend::default();
        spend.add(role, usd);
        Self {
            calls: std::sync::Mutex::default(),
            spend,
        }
    }

    pub fn recorded(&self) -> Vec<LlmCallRecord> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl LlmUsageSink for TestSink {
    fn record(&self, call: LlmCallRecord) {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(call);
    }

    fn spent_today(&self) -> RoleSpend {
        self.spend
    }
}
