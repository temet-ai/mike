mod as_of;
mod parity;
mod run;
mod sink;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_db;
#[cfg(test)]
mod tests_visibility;

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

pub(crate) use parity::{RecordedCall, parity_recorded};
pub(crate) use run::{ItemReport, ReplayMode, ReplaySteerer, load_replay_steerer, replay_items};
pub(crate) use sink::FileSink;

use crate::llm::limiter::{CallLimiter, LimiterLimits};
use crate::llm::{LlmError, OpenRouterClient};

const REPLAY_CALLS_PER_WINDOW: u32 = 100_000;

#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct ReplayItem {
    pub(crate) turn_id: i64,
    pub(crate) trigger_request_id: i64,
}

pub(crate) fn replay_client(
    api_key: String,
    model: String,
    base_url: Option<String>,
    timeout: Option<Duration>,
) -> Result<OpenRouterClient, LlmError> {
    let limiter = CallLimiter::with_limits(LimiterLimits::fixed(
        REPLAY_CALLS_PER_WINDOW,
        Duration::from_secs(60),
        REPLAY_CALLS_PER_WINDOW,
        Duration::from_secs(3600),
    ));
    OpenRouterClient::new_with_limiter(api_key, model, base_url, timeout, Arc::new(limiter))
}
