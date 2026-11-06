pub(crate) mod backoff;
pub(crate) mod budget;
mod child_stdin;
pub(crate) mod claude;
pub(crate) mod codex;
pub mod error;
pub(crate) mod health;
pub(crate) mod key;
pub(crate) mod limiter;
pub(crate) mod model_ref;
pub mod openrouter;
pub mod outage;
pub(crate) mod provider;
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
