use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::call::{CallUsage, extract_usage, parse_content};
use super::recording::RecordFields;
use super::{LlmCallOutcome, LlmError, LlmRuntimeSettings, OpenRouterClient};
use crate::llm::model_ref::Provider;
use crate::llm::provider::local::LOCAL_BUSY;
use crate::llm::provider::{LocalEndpoint, local_endpoint};

const SLOTS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const BUSY_SLOTS: usize = 2;

struct LocalReply {
    status: reqwest::StatusCode,
    body: String,
}

fn local_body(
    model: &str,
    system: &str,
    user: &str,
    schema: &Value,
    live: Option<&LlmRuntimeSettings>,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "verdict",
                "strict": true,
                "schema": schema
            }
        },
        "chat_template_kwargs": {"enable_thinking": false}
    });
    if let Some(live) = live.filter(|s| s.max_output_tokens > 0) {
        body["max_tokens"] = json!(live.max_output_tokens);
    }
    body
}

fn transport_error(e: reqwest::Error) -> LlmError {
    if e.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Transport(e.to_string())
    }
}

impl OpenRouterClient {
    fn local_unavailable(&self) -> LlmError {
        LlmError::ProviderUnavailable {
            model: self.inner.model.clone(),
            provider: Provider::Local.display_name(),
            missing: "llm.local.url is not set; set it at /settings to the base URL of the llama-server or Ollama instance, for example http://mac.local:8080/v1".to_owned(),
        }
    }

    fn local_busy_error(&self) -> LlmError {
        LlmError::ProviderUnavailable {
            model: self.inner.model.clone(),
            provider: Provider::Local.display_name(),
            missing: LOCAL_BUSY.to_owned(),
        }
    }

    async fn local_is_busy(&self, endpoint: &LocalEndpoint) -> bool {
        let Some(url) = endpoint.slots_url() else {
            return false;
        };
        let mut request = self.inner.http.get(url).timeout(SLOTS_TIMEOUT);
        let key = endpoint.api_key();
        if !key.is_empty() {
            request = request.bearer_auth(key);
        }
        let Ok(resp) = request.send().await else {
            return false;
        };
        if !resp.status().is_success() {
            return false;
        }
        let Ok(slots) = resp.json::<Value>().await else {
            return false;
        };
        slots.as_array().is_some_and(|slots| {
            slots
                .iter()
                .filter(|slot| slot.get("is_processing").and_then(Value::as_bool) == Some(true))
                .count()
                >= BUSY_SLOTS
        })
    }

    async fn post_local(
        &self,
        endpoint: &LocalEndpoint,
        base_url: &str,
        body: &Value,
    ) -> Result<LocalReply, LlmError> {
        let mut request = self
            .inner
            .http
            .post(format!("{base_url}/chat/completions"))
            .timeout(self.inner.timeout)
            .json(body);
        let key = endpoint.api_key();
        if !key.is_empty() {
            request = request.bearer_auth(key);
        }
        let resp = request.send().await.map_err(transport_error)?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        Ok(LocalReply { status, body })
    }

    pub async fn attempt_local_verdict<T: DeserializeOwned>(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
        live: Option<&LlmRuntimeSettings>,
    ) -> Result<(T, CallUsage), LlmError> {
        let endpoint = local_endpoint();
        let Some(base_url) = endpoint.base_url().map(str::to_owned) else {
            return Err(self.local_unavailable());
        };
        if self.local_is_busy(&endpoint).await {
            return Err(self.local_busy_error());
        }
        let started = Instant::now();
        let sent_body = local_body(model, system, user, schema, live);
        let reply = match self.post_local(&endpoint, &base_url, &sent_body).await {
            Ok(reply) => reply,
            Err(e) => {
                self.record_local_error(&e, None, started, &sent_body);
                return Err(e);
            }
        };
        if !reply.status.is_success() {
            let err = LlmError::Http {
                status: reply.status.as_u16(),
            };
            self.record_local_error(&err, Some(reply.status.as_u16()), started, &sent_body);
            return Err(err);
        }
        self.finish_local_verdict(&reply.body, live, started, &sent_body)
    }

    fn record_local_error(
        &self,
        error: &LlmError,
        http_status: Option<u16>,
        started: Instant,
        sent_body: &Value,
    ) {
        self.record_error_if_known(
            error,
            RecordFields {
                usage: None,
                generation_id: None,
                http_status,
                latency: started.elapsed(),
                request_body: Some(sent_body),
            },
        );
    }

    fn finish_local_verdict<T: DeserializeOwned>(
        &self,
        raw_body: &str,
        live: Option<&LlmRuntimeSettings>,
        started: Instant,
        sent_body: &Value,
    ) -> Result<(T, CallUsage), LlmError> {
        let payload: Value = serde_json::from_str(raw_body)?;
        let generation_id = payload.get("id").and_then(Value::as_str).map(str::to_owned);
        let mut usage = extract_usage(&payload);
        usage.cost_usd = Some(0.0);
        let fields = || RecordFields {
            usage: Some(&usage),
            generation_id: generation_id.as_deref(),
            http_status: Some(200),
            latency: started.elapsed(),
            request_body: Some(sent_body),
        };
        if let Some(err) = self.check_truncation(&payload, live, fields()) {
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
        self.record_if_configured(LlmCallOutcome::Ok, fields());
        Ok((result, call_usage))
    }
}
