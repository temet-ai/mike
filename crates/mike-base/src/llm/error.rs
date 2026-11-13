use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("LLM request timed out")]
    Timeout,

    #[error("LLM HTTP error: status {status}")]
    Http { status: u16 },

    #[error("LLM response schema error: {0}")]
    Schema(#[from] serde_json::Error),

    #[error("LLM transport error: {0}")]
    Transport(String),

    #[error("OpenRouter call ceiling exceeded ({limit} calls per {window_secs}s)")]
    CeilingExceeded { limit: u32, window_secs: u64 },

    #[error("OpenRouter call rate limit exceeded ({limit}/min)")]
    RateLimited { limit: u32 },

    #[error(
        "LLM response truncated by max_tokens={max_output_tokens} (finish_reason: length) — raise llm.max_output_tokens"
    )]
    OutputTruncated { max_output_tokens: u32 },

    #[error(
        "OpenRouter daily spend ceiling reached for this role: spent ${spent_usd:.4} against a guaranteed share of ${share_usd:.4}, and the shared remainder of the ${ceiling_usd:.2} daily ceiling is used up"
    )]
    SpendCeilingExceeded {
        spent_usd: f64,
        share_usd: f64,
        ceiling_usd: f64,
    },

    #[error("model `{model}` runs on {provider}, which is unavailable: {missing}")]
    ProviderUnavailable {
        model: String,
        provider: &'static str,
        missing: String,
    },
}

impl LlmError {
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            LlmError::CeilingExceeded { .. }
                | LlmError::RateLimited { .. }
                | LlmError::SpendCeilingExceeded { .. }
                | LlmError::ProviderUnavailable { .. }
        )
    }

    pub fn is_credential_failure(&self) -> bool {
        matches!(self, LlmError::Http { status: 401..=403 })
    }
}
