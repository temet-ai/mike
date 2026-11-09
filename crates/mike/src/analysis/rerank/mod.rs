use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::deserialize_permissive_bool;

#[cfg(test)]
mod cold_start_tests;
mod runtime;
mod service;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod settings_tests;

pub(crate) use runtime::{RerankPhase, RerankStatus};
pub use service::RerankService;

pub(crate) const DEFAULT_RERANK_MODEL: &str = "bge-reranker-v2-m3";

pub(crate) const DEFAULT_RERANK_URL: &str = "http://127.0.0.1:11435";

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 800;

pub(crate) const DEFAULT_CANDIDATES: usize = 50;

pub(crate) const HEALTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(crate) const MAX_DOCUMENT_CHARS: usize = 6_000;

pub(crate) const MAX_QUERY_CHARS: usize = 2_000;

pub(crate) const COLD_AFTER_SECS: i64 = 600;

pub(crate) const BREAKER_THRESHOLD: u32 = 3;

pub(crate) const BREAKER_BASE_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(15);

pub(crate) const BREAKER_MAX_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RerankConfig {
    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub enabled: bool,

    pub url: String,

    pub model: String,

    pub timeout_ms: u64,

    pub candidates: usize,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub tuning_enabled: bool,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: DEFAULT_RERANK_URL.to_owned(),
            model: DEFAULT_RERANK_MODEL.to_owned(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
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
