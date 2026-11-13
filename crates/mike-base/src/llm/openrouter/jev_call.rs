use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::call::UsageInfo;
use super::{LlmCallOutcome, LlmCallRecord, LlmError, OpenRouterClient};
use crate::llm::CallUsage;
use crate::llm::decisions::{
    DecisionRequest, DecisionResponse, WireResponse, decisions_url, parse_response,
};

struct Sent {
    status: u16,
    body: String,
}

struct Trace<'a> {
    started: Instant,
    request: &'a Value,
}

fn refusal_outcome(e: &LlmError) -> Option<LlmCallOutcome> {
    match e {
        LlmError::CeilingExceeded { .. } => Some(LlmCallOutcome::RefusedCeiling),
        LlmError::RateLimited { .. } => Some(LlmCallOutcome::RefusedRate),
        LlmError::SpendCeilingExceeded { .. } => Some(LlmCallOutcome::RefusedSpend),
        _ => None,
    }
}

fn failure_outcome(e: &LlmError) -> LlmCallOutcome {
    match e {
        LlmError::Timeout => LlmCallOutcome::Timeout,
        LlmError::Http { .. } => LlmCallOutcome::HttpError,
        LlmError::Schema(_) => LlmCallOutcome::SchemaError,
        _ => LlmCallOutcome::TransportError,
    }
}

fn usage_of(wire: &WireResponse) -> UsageInfo {
    let as_i32 = |n: Option<i64>| n.and_then(|v| i32::try_from(v).ok());
    UsageInfo {
        prompt_tokens: as_i32(wire.usage.input_tokens),
        completion_tokens: as_i32(wire.usage.output_tokens),
        reasoning_tokens: None,
        cached_tokens: None,
        cost_usd: wire.usage.cost,
    }
}

fn stored_answers(wire: &WireResponse) -> Value {
    json!({ "id": wire.id, "model": wire.model, "answers": wire.answers })
}

fn body_as_json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.to_owned()))
}

impl OpenRouterClient {
    pub async fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, LlmError> {
        self.require_api_key()?;
        let request = serde_json::to_value(req)?;
        if let Err(e) = self
            .inner
            .limiter
            .admit(self.inner.role, self.inner.reservation.as_ref())
        {
            if let Some(outcome) = refusal_outcome(&e) {
                self.record_decision(
                    outcome,
                    None,
                    None,
                    None,
                    Trace {
                        started: Instant::now(),
                        request: &request,
                    },
                );
            }
            return Err(e);
        }
        let started = Instant::now();
        let trace = Trace {
            started,
            request: &request,
        };
        let sent = match self.post_decision(&request).await {
            Err(LlmError::Timeout | LlmError::Transport(_)) => self.post_decision(&request).await,
            other => other,
        };
        let sent = match sent {
            Ok(sent) => sent,
            Err(e) => {
                self.record_decision(failure_outcome(&e), None, None, None, trace);
                return Err(e);
            }
        };
        if !(200..300).contains(&sent.status) {
            let e = LlmError::Http {
                status: sent.status,
            };
            let body = body_as_json(&sent.body);
            self.record_decision(
                failure_outcome(&e),
                Some(sent.status),
                None,
                Some(body),
                trace,
            );
            return Err(e);
        }
        let wire = match parse_response(&sent.body, req) {
            Ok(wire) => wire,
            Err(e) => {
                let body = body_as_json(&sent.body);
                self.record_decision(
                    LlmCallOutcome::SchemaError,
                    Some(200),
                    None,
                    Some(body),
                    trace,
                );
                return Err(e);
            }
        };
        let usage = usage_of(&wire);
        self.record_decision(
            LlmCallOutcome::Ok,
            Some(200),
            Some((&usage, wire.id.as_deref())),
            Some(stored_answers(&wire)),
            trace,
        );
        Ok(DecisionResponse {
            usage: CallUsage {
                generation_id: wire.id.clone(),
                cost_usd: usage.cost_usd,
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                latency_ms: i32::try_from(started.elapsed().as_millis()).ok(),
            },
            model: wire.model,
            answers: wire.answers,
        })
    }

    pub fn decisions_endpoint(&self) -> String {
        decisions_url(&self.inner.base_url)
    }

    pub fn timeout(&self) -> Duration {
        self.inner.timeout
    }

    pub fn with_timeout(&self, timeout: Duration) -> Self {
        let mut inner = (*self.inner).clone();
        inner.timeout = timeout;
        Self {
            inner: std::sync::Arc::new(inner),
        }
    }

    async fn post_decision(&self, body: &Value) -> Result<Sent, LlmError> {
        let resp = self
            .inner
            .http
            .post(self.decisions_endpoint())
            .bearer_auth(&self.inner.api_key)
            .header("HTTP-Referer", "https://github.com/temet-ai/mike")
            .header("X-Title", "mike decisions")
            .timeout(self.inner.timeout)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Transport(e.to_string())
                }
            })?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        Ok(Sent { status, body })
    }

    fn record_decision(
        &self,
        outcome: LlmCallOutcome,
        http_status: Option<u16>,
        usage: Option<(&UsageInfo, Option<&str>)>,
        response_body: Option<Value>,
        trace: Trace<'_>,
    ) {
        let Some(sink) = &self.inner.usage_sink else {
            return;
        };
        let info = usage.map(|(u, _)| u);
        sink.record(LlmCallRecord {
            role: self.inner.role,
            model: response_model(response_body.as_ref()).unwrap_or_else(|| self.wire_model()),
            generation_id: usage.and_then(|(_, id)| id).map(str::to_owned),
            prompt_tokens: info.and_then(|u| u.prompt_tokens),
            completion_tokens: info.and_then(|u| u.completion_tokens),
            reasoning_tokens: None,
            cached_tokens: None,
            cost_usd: info.and_then(|u| u.cost_usd),
            latency_ms: i32::try_from(trace.started.elapsed().as_millis()).ok(),
            outcome,
            http_status: http_status.map(i32::from),
            session_id: self.inner.call_context.session_id,
            steerer_id: self.inner.call_context.steerer_id,
            operator_turn_id: self.inner.call_context.operator_turn_id,
            request_body: Some(trace.request.clone()),
            response_body,
        });
    }
}

fn response_model(body: Option<&Value>) -> Option<String> {
    body.and_then(|b| b.get("model"))
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
}
