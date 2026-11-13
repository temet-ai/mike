use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::call::UsageInfo;
use super::{LlmCallOutcome, LlmCallRecord, LlmError, OpenRouterClient};
use crate::llm::CallUsage;
use crate::llm::decisions::record::{CallResult, DecisionContext, RecordInsert};
use crate::llm::decisions::{
    DecisionOutcome, DecisionRequest, DecisionResponse, WireResponse, decisions_url, parse_response,
};
use crate::store::persistence::PersistHandle;

struct Sent {
    status: u16,
    body: String,
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

fn body_as_json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.to_owned()))
}

fn elapsed_ms(started: Instant) -> Option<i32> {
    i32::try_from(started.elapsed().as_millis()).ok()
}

fn failed(outcome: &'static str, e: &LlmError, started: Instant) -> CallResult {
    CallResult {
        outcome,
        error: Some(e.to_string()),
        latency_ms: elapsed_ms(started),
        ..CallResult::default()
    }
}

fn answered(wire: &WireResponse, raw: String, started: Instant) -> CallResult {
    CallResult {
        outcome: "ok",
        response_raw: Some(raw),
        answers: serde_json::to_value(&wire.answers).ok(),
        question_refusals: i32::try_from(wire.answers.values().filter(|a| a.is_refusal()).count())
            .unwrap_or(i32::MAX),
        served_model: Some(wire.model.clone()),
        provider: wire.provider.clone(),
        generation_id: wire.id.clone(),
        usage: serde_json::to_value(&wire.usage).ok(),
        cost_usd: wire.usage.cost,
        latency_ms: elapsed_ms(started),
        http_status: Some(200),
        error: None,
    }
}

impl OpenRouterClient {
    pub fn with_decision_store(&self, store: PersistHandle) -> Self {
        let mut inner = (*self.inner).clone();
        inner.decision_store = Some(store);
        Self {
            inner: std::sync::Arc::new(inner),
        }
    }

    pub async fn decide(&self, req: &DecisionRequest, ctx: &DecisionContext) -> DecisionOutcome {
        let request = serde_json::to_value(req).unwrap_or(Value::Null);
        let started = Instant::now();
        let (call, result) = self.call_decision(req, &request, started).await;
        let record_id = self.store_decision(req, &request, ctx, &call).await;
        DecisionOutcome { record_id, result }
    }

    async fn call_decision(
        &self,
        req: &DecisionRequest,
        request: &Value,
        started: Instant,
    ) -> (CallResult, Result<DecisionResponse, LlmError>) {
        if let Err(e) = self.require_api_key() {
            return (failed("no_key", &e, started), Err(e));
        }
        if let Err(e) = self
            .inner
            .limiter
            .admit(self.inner.role, self.inner.reservation.as_ref())
        {
            if let Some(outcome) = refusal_outcome(&e) {
                self.record_spend(outcome, None, None, None, request, started);
                return (failed(outcome.as_str(), &e, started), Err(e));
            }
            return (failed("transport_error", &e, started), Err(e));
        }
        let sent = match self.post_decision(request).await {
            Err(LlmError::Timeout | LlmError::Transport(_)) => self.post_decision(request).await,
            other => other,
        };
        let sent = match sent {
            Ok(sent) => sent,
            Err(e) => {
                let outcome = failure_outcome(&e);
                self.record_spend(outcome, None, None, None, request, started);
                return (failed(outcome.as_str(), &e, started), Err(e));
            }
        };
        self.read_reply(req, request, sent, started)
    }

    fn read_reply(
        &self,
        req: &DecisionRequest,
        request: &Value,
        sent: Sent,
        started: Instant,
    ) -> (CallResult, Result<DecisionResponse, LlmError>) {
        let status = Some(i32::from(sent.status));
        if !(200..300).contains(&sent.status) {
            let e = LlmError::Http {
                status: sent.status,
            };
            let body = body_as_json(&sent.body);
            self.record_spend(
                failure_outcome(&e),
                Some(sent.status),
                None,
                Some(body),
                request,
                started,
            );
            let call = CallResult {
                response_raw: Some(sent.body),
                http_status: status,
                ..failed("http_error", &e, started)
            };
            return (call, Err(e));
        }
        let wire = match parse_response(&sent.body, req) {
            Ok(wire) => wire,
            Err(e) => {
                let body = body_as_json(&sent.body);
                self.record_spend(
                    LlmCallOutcome::SchemaError,
                    Some(200),
                    None,
                    Some(body),
                    request,
                    started,
                );
                let call = CallResult {
                    response_raw: Some(sent.body),
                    http_status: status,
                    ..failed("schema_error", &e, started)
                };
                return (call, Err(e));
            }
        };
        let usage = usage_of(&wire);
        let stored = json!({ "id": wire.id, "model": wire.model, "answers": wire.answers });
        self.record_spend(
            LlmCallOutcome::Ok,
            Some(200),
            Some((&usage, wire.id.as_deref())),
            Some(stored),
            request,
            started,
        );
        let call = answered(&wire, sent.body, started);
        let response = DecisionResponse {
            usage: CallUsage {
                generation_id: wire.id.clone(),
                cost_usd: usage.cost_usd,
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                latency_ms: elapsed_ms(started),
            },
            model: wire.model,
            answers: wire.answers,
        };
        (call, Ok(response))
    }

    async fn store_decision(
        &self,
        req: &DecisionRequest,
        request: &Value,
        ctx: &DecisionContext,
        call: &CallResult,
    ) -> Option<i64> {
        let store = self.inner.decision_store.as_ref()?;
        let questions = serde_json::to_value(&req.questions).unwrap_or(Value::Null);
        let row = RecordInsert {
            context: ctx,
            model_requested: &req.model,
            state: &req.state,
            questions: &questions,
            request_body: request,
            result: call,
            created_at: None,
        };
        match store.insert_decision_record(row).await {
            Ok(id) => Some(id),
            Err(error) => {
                tracing::warn!(%error, point = ctx.point, "decisions: failed to record a decision");
                None
            }
        }
    }

    pub fn decisions_endpoint(&self) -> String {
        decisions_url(&self.inner.base_url)
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

    fn record_spend(
        &self,
        outcome: LlmCallOutcome,
        http_status: Option<u16>,
        usage: Option<(&UsageInfo, Option<&str>)>,
        response_body: Option<Value>,
        request: &Value,
        started: Instant,
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
            latency_ms: elapsed_ms(started),
            outcome,
            http_status: http_status.map(i32::from),
            session_id: self.inner.call_context.session_id,
            steerer_id: self.inner.call_context.steerer_id,
            operator_turn_id: self.inner.call_context.operator_turn_id,
            request_body: Some(request.clone()),
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
