use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::call::CallUsage;
use super::{LlmError, LlmRuntimeSettings, OpenRouterClient};
use crate::llm::codex::{self, CodexRequest};

impl OpenRouterClient {
    pub(super) async fn attempt_codex_verdict<T: DeserializeOwned>(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
        live: Option<&LlmRuntimeSettings>,
    ) -> Result<(T, CallUsage), LlmError> {
        let started = Instant::now();
        let reasoning_effort = live.map_or("", |s| s.reasoning_effort.as_str());
        let sent_body = self.cli_sent_body(
            "codex",
            system,
            user,
            codex::codex_reasoning_effort(reasoning_effort),
        );
        let request = CodexRequest {
            model,
            system,
            user,
            schema,
            reasoning_effort,
            timeout: codex::DEFAULT_TIMEOUT,
        };
        let outcome = codex::run(codex::DEFAULT_BIN, request).await;
        self.finish_cli_verdict(started, &sent_body, outcome)
    }
}
