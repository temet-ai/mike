use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::call::CallUsage;
use super::{LlmError, LlmRuntimeSettings, OpenRouterClient};
use crate::llm::claude::{self, ClaudeRequest};

impl OpenRouterClient {
    pub(super) async fn attempt_claude_verdict<T: DeserializeOwned>(
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
            "claude",
            system,
            user,
            claude::claude_effort(reasoning_effort).unwrap_or("none"),
        );
        let request = ClaudeRequest {
            model,
            system,
            user,
            schema,
            reasoning_effort,
            timeout: claude::DEFAULT_TIMEOUT,
        };
        let outcome = claude::run(&claude::binary(), request).await;
        self.finish_cli_verdict(started, &sent_body, outcome)
    }
}
