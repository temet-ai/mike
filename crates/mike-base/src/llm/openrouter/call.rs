use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::recording::RecordFields;
use super::{LlmError, LlmRuntimeSettings, OpenRouterClient};
use crate::llm::model_ref::{ModelRef, Provider};

pub fn is_retryable(e: &LlmError) -> bool {
    matches!(
        e,
        LlmError::Schema(_) | LlmError::Transport(_) | LlmError::Timeout
    )
}

struct RawResponse {
    status: reqwest::StatusCode,
    body: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UsageInfo {
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub reasoning_tokens: Option<i32>,
    pub cached_tokens: Option<i32>,
    pub cost_usd: Option<f64>,
}

pub fn extract_usage(payload: &Value) -> UsageInfo {
    let usage = payload.get("usage");
    let as_i32 = |v: Option<&Value>| {
        v.and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
    };
    UsageInfo {
        prompt_tokens: as_i32(usage.and_then(|u| u.get("prompt_tokens"))),
        completion_tokens: as_i32(usage.and_then(|u| u.get("completion_tokens"))),
        reasoning_tokens: as_i32(
            usage
                .and_then(|u| u.get("completion_tokens_details"))
                .and_then(|d| d.get("reasoning_tokens")),
        ),
        cached_tokens: as_i32(
            usage
                .and_then(|u| u.get("prompt_tokens_details"))
                .and_then(|d| d.get("cached_tokens")),
        ),
        cost_usd: usage.and_then(|u| u.get("cost")).and_then(Value::as_f64),
    }
}

fn is_provider_eligibility_error(status: reqwest::StatusCode, body: &str) -> bool {
    matches!(status.as_u16(), 400 | 404) && {
        let lower = body.to_ascii_lowercase();
        lower.contains("provider") || lower.contains("parameter")
    }
}

enum Resend {
    LowEffort(reqwest::StatusCode),
    WithoutReasoning(reqwest::StatusCode),
}

fn resend_kind(sent_body: &Value, raw: &RawResponse, include_reasoning: bool) -> Option<Resend> {
    if is_reasoning_mandatory_error(raw.status, &raw.body)
        && sent_reasoning_effort_is_none(sent_body)
    {
        return Some(Resend::LowEffort(raw.status));
    }
    if !raw.status.is_success()
        && include_reasoning
        && is_provider_eligibility_error(raw.status, &raw.body)
    {
        return Some(Resend::WithoutReasoning(raw.status));
    }
    None
}

pub fn is_reasoning_mandatory_error(status: reqwest::StatusCode, body: &str) -> bool {
    status.is_client_error() && body.to_ascii_lowercase().contains("reasoning is mandatory")
}

pub fn sent_reasoning_effort_is_none(body: &Value) -> bool {
    body.pointer("/reasoning/effort").and_then(Value::as_str) == Some("none")
}

pub fn parse_content<T: DeserializeOwned>(payload: &Value) -> Result<T, LlmError> {
    let content = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LlmError::Transport("OpenRouter response missing choices[0].message.content".to_owned())
        })?;

    Ok(serde_json::from_str(content)?)
}

impl OpenRouterClient {
    fn build_body(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        live: Option<&LlmRuntimeSettings>,
        include_reasoning: bool,
    ) -> Value {
        let mut body = json!({
            "model": self.wire_model(),
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "steerer_verdict",
                    "strict": true,
                    "schema": schema
                }
            },
            "provider": {
                "require_parameters": true
            }
        });

        if let Some(live) = live {
            if include_reasoning {
                body["reasoning"] = json!({ "effort": live.reasoning_effort });
            }
            if live.max_output_tokens > 0 {
                body["max_tokens"] = json!(live.max_output_tokens);
            }
        }

        body
    }

    async fn send_raw(&self, body: &Value) -> Result<RawResponse, LlmError> {
        let url = format!("{}/chat/completions", self.inner.base_url);

        let resp = self
            .inner
            .http
            .post(&url)
            .bearer_auth(&self.inner.api_key)
            .header("HTTP-Referer", "https://github.com/temet-ai/mike")
            .header("X-Title", "mike steerer")
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

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        Ok(RawResponse { status, body })
    }

    async fn send_with_eligibility_retry(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        live: Option<&LlmRuntimeSettings>,
    ) -> (Value, Result<RawResponse, LlmError>) {
        let include_reasoning = live.is_some();
        let sent_body = self.build_body(system, user, schema, live, include_reasoning);
        let raw = self.send_raw(&sent_body).await;
        let resend = raw
            .as_ref()
            .ok()
            .and_then(|r| resend_kind(&sent_body, r, include_reasoning));
        match resend {
            Some(Resend::LowEffort(status)) => self.resend_with_low_effort(sent_body, status).await,
            Some(Resend::WithoutReasoning(status)) => {
                self.resend_without_reasoning(system, user, schema, live, status)
                    .await
            }
            None => (sent_body, raw),
        }
    }

    async fn resend_with_low_effort(
        &self,
        mut body: Value,
        status: reqwest::StatusCode,
    ) -> (Value, Result<RawResponse, LlmError>) {
        tracing::warn!(
            status = status.as_u16(),
            model = %self.inner.model,
            "openrouter: retrying once with reasoning effort `low` — model refuses `none`"
        );
        body["reasoning"] = json!({ "effort": "low" });
        let retried = self.send_raw(&body).await;
        (body, retried)
    }

    async fn resend_without_reasoning(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        live: Option<&LlmRuntimeSettings>,
        status: reqwest::StatusCode,
    ) -> (Value, Result<RawResponse, LlmError>) {
        tracing::warn!(
            status = status.as_u16(),
            "openrouter: retrying once without `reasoning` — provider eligibility error"
        );
        let body = self.build_body(system, user, schema, live, false);
        let retried = self.send_raw(&body).await;
        (body, retried)
    }

    fn check_truncation(
        &self,
        payload: &Value,
        live: Option<&LlmRuntimeSettings>,
        fields: RecordFields<'_>,
    ) -> Option<LlmError> {
        let finish_reason = payload
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str);
        if finish_reason != Some("length") {
            return None;
        }
        let cap = live.map_or(0, |s| s.max_output_tokens);
        let err = LlmError::OutputTruncated {
            max_output_tokens: cap,
        };
        tracing::warn!(
            max_output_tokens = cap,
            "openrouter: response truncated by max_tokens — raise llm.max_output_tokens"
        );
        self.record_error_if_known(&err, fields);
        Some(err)
    }

    pub async fn attempt_verdict<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
    ) -> Result<(T, CallUsage), LlmError> {
        let live_owned = self.effective_settings();
        let live = live_owned.as_ref();
        let target = ModelRef::parse(&self.inner.model);
        if target.provider.needs_openrouter_key() {
            self.require_api_key()?;
        }
        self.inner
            .limiter
            .admit(self.inner.role, self.inner.reservation.as_ref())?;

        match target.provider {
            Provider::Codex => {
                return self
                    .attempt_codex_verdict(&target.model, system, user, schema, live)
                    .await;
            }
            Provider::Claude => {
                return self
                    .attempt_claude_verdict(&target.model, system, user, schema, live)
                    .await;
            }
            Provider::OpenRouter => {}
        }

        let started = Instant::now();
        let (sent_body, raw) = self
            .send_with_eligibility_retry(system, user, schema, live)
            .await;

        let raw = match raw {
            Ok(raw) => raw,
            Err(e) => {
                self.record_error_if_known(
                    &e,
                    RecordFields {
                        usage: None,
                        generation_id: None,
                        http_status: None,
                        latency: started.elapsed(),
                        request_body: Some(&sent_body),
                    },
                );
                return Err(e);
            }
        };

        if !raw.status.is_success() {
            let err = LlmError::Http {
                status: raw.status.as_u16(),
            };
            self.record_error_if_known(
                &err,
                RecordFields {
                    usage: None,
                    generation_id: None,
                    http_status: Some(raw.status.as_u16()),
                    latency: started.elapsed(),
                    request_body: Some(&sent_body),
                },
            );
            return Err(err);
        }

        let payload: Value = serde_json::from_str(&raw.body)?;

        let generation_id = payload.get("id").and_then(Value::as_str).map(str::to_owned);
        let usage = extract_usage(&payload);

        if let Some(err) = self.check_truncation(
            &payload,
            live,
            RecordFields {
                usage: Some(&usage),
                generation_id: generation_id.as_deref(),
                http_status: Some(200),
                latency: started.elapsed(),
                request_body: Some(&sent_body),
            },
        ) {
            return Err(err);
        }

        let result: T = parse_content(&payload)?;
        let call_usage = CallUsage {
            generation_id: generation_id.clone(),
            cost_usd: usage.cost_usd,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            latency_ms: i32::try_from(started.elapsed().as_millis()).ok(),
        };

        self.record_if_configured(
            super::LlmCallOutcome::Ok,
            RecordFields {
                usage: Some(&usage),
                generation_id: generation_id.as_deref(),
                http_status: Some(200),
                latency: started.elapsed(),
                request_body: Some(&sent_body),
            },
        );
        Ok((result, call_usage))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallUsage {
    pub generation_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
}
