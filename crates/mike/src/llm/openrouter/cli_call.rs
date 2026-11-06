use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::call::{CallUsage, extract_usage, parse_content};
use super::recording::RecordFields;
use super::{LlmCallOutcome, LlmError, OpenRouterClient};

impl OpenRouterClient {
    pub(super) fn cli_sent_body(
        &self,
        transport: &str,
        system: &str,
        user: &str,
        reasoning_effort: &str,
    ) -> Value {
        json!({
            "model": self.inner.model,
            "transport": transport,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "reasoning": {"effort": reasoning_effort}
        })
    }

    pub(super) fn finish_cli_verdict<T: DeserializeOwned>(
        &self,
        started: Instant,
        sent_body: &Value,
        outcome: Result<Value, LlmError>,
    ) -> Result<(T, CallUsage), LlmError> {
        let payload = match outcome {
            Ok(payload) => payload,
            Err(e) => {
                self.record_error_if_known(
                    &e,
                    RecordFields {
                        usage: None,
                        generation_id: None,
                        http_status: None,
                        latency: started.elapsed(),
                        request_body: Some(sent_body),
                    },
                );
                return Err(e);
            }
        };

        let generation_id = payload.get("id").and_then(Value::as_str).map(str::to_owned);
        let usage = extract_usage(&payload);
        let result: T = parse_content(&payload)?;
        let call_usage = CallUsage {
            generation_id: generation_id.clone(),
            cost_usd: usage.cost_usd,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            latency_ms: i32::try_from(started.elapsed().as_millis()).ok(),
        };
        self.record_if_configured(
            LlmCallOutcome::Ok,
            RecordFields {
                usage: Some(&usage),
                generation_id: generation_id.as_deref(),
                http_status: None,
                latency: started.elapsed(),
                request_body: Some(sent_body),
            },
        );
        Ok((result, call_usage))
    }
}
