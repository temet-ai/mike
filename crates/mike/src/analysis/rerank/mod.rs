use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::deserialize_permissive_bool;

#[cfg(test)]
mod cold_start_tests;
mod keys;
#[cfg(test)]
mod keys_tests;
mod nightly;
#[cfg(test)]
mod queue_tests;
mod runtime;
mod service;
#[cfg(test)]
mod service_score_cache_tests;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod settings_tests;
pub(crate) mod warmup;
#[cfg(test)]
mod warmup_tests;

pub(crate) use keys::assign_rerank_key;
pub use nightly::RerankNightly;
pub(crate) use nightly::{DEFAULT_RESCUE_MIN_KEPT, DEFAULT_RESCUE_SCORE};
pub(crate) use runtime::{RerankPhase, RerankStatus};
pub use service::RerankService;
pub(crate) use service::Reranked;

pub(crate) const DEFAULT_RERANK_MODEL: &str = "bge-reranker-v2-m3";

pub(crate) const DEFAULT_RERANK_URL: &str = "http://127.0.0.1:11435";

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 800;

pub(crate) const DEFAULT_COLD_TIMEOUT_MS: u64 = 6_000;

pub(crate) const DEFAULT_MIN_QUERY_WORDS: usize = 1;

pub(crate) const DEFAULT_CANDIDATES: usize = 50;

pub(crate) const HEALTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(crate) const MAX_DOCUMENT_CHARS: usize = 600;

pub(crate) const MAX_QUERY_CHARS: usize = 600;

pub(crate) const COLD_AFTER_SECS: i64 = 600;

pub(crate) const BREAKER_THRESHOLD: u32 = 3;

pub(crate) const BREAKER_BASE_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(15);

pub(crate) const BREAKER_MAX_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RerankConfig {
    pub rescue_score: f64,

    pub rescue_min_kept: usize,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub search_no_project: bool,

    pub nightly: RerankNightly,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub enabled: bool,

    pub url: String,

    pub model: String,

    pub timeout_ms: u64,

    pub cold_timeout_ms: u64,

    pub min_query_words: usize,

    pub candidates: usize,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub tuning_enabled: bool,
}

impl RerankConfig {
    pub(crate) fn call_budget(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.cold_timeout_ms.saturating_add(self.timeout_ms))
    }
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            rescue_score: DEFAULT_RESCUE_SCORE,
            rescue_min_kept: DEFAULT_RESCUE_MIN_KEPT,
            search_no_project: true,
            nightly: RerankNightly::default(),
            enabled: false,
            url: DEFAULT_RERANK_URL.to_owned(),
            model: DEFAULT_RERANK_MODEL.to_owned(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            cold_timeout_ms: DEFAULT_COLD_TIMEOUT_MS,
            min_query_words: DEFAULT_MIN_QUERY_WORDS,
            candidates: DEFAULT_CANDIDATES,
            tuning_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHealth {
    Off,
    Ready,
    Loading,
    Unavailable(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RerankError {
    #[error("reranker is disabled by configuration (rerank.enabled is off)")]
    Disabled,

    #[error(
        "rerank refused before any request: {0} is not a loopback host and remote egress is not allowed"
    )]
    Refused(String),

    #[error("rerank skipped: the circuit breaker is open after repeated failures")]
    BreakerOpen,

    #[error("the reranker is still loading its model")]
    Loading,

    #[error("rerank request timed out after {0} ms")]
    Timeout(u64),

    #[error("rerank HTTP error: {0}")]
    Http(String),

    #[error("rerank response parse error: {0}")]
    Parse(String),
}
