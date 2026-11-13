pub mod backoff;
pub mod budget;
pub mod child_stdin;
pub mod claude;
pub mod codex;
pub mod decisions;
pub mod error;
pub mod health;
pub mod key;
pub mod limiter;
pub mod model_ref;
pub mod openrouter;
pub mod outage;
pub mod provider;
pub mod runtime;
pub mod usage;

pub use error::LlmError;
pub use limiter::LimiterSnapshot;
#[cfg(any(test, feature = "test-seams"))]
pub use limiter::{exhaust_global_rate_for_tests, reset_global_for_tests};
pub use openrouter::{CallUsage, OpenRouterClient};
pub use runtime::{LiveSettingsSource, LlmRuntimeSettings};
pub use usage::{
    DailySpendLedger, LlmCallContext, LlmCallOutcome, LlmCallRecord, LlmRole, LlmUsageSink,
    NoopUsageSink, ROLE_COUNT, RoleSpend,
};
