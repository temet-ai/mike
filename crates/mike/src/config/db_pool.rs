use serde::{Deserialize, Serialize};

use crate::store::persistence::pool_classes::{
    DEFAULT_BACKGROUND_LIMITS, DEFAULT_CAPTURE_LIMITS, DEFAULT_INTERACTIVE_LIMITS, PoolClassLimits,
    PoolClassesConfig,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbPoolConfig {
    #[serde(rename = "db_pool_capture_size")]
    pub(crate) capture_size: u32,

    #[serde(rename = "db_pool_capture_acquire_timeout_ms")]
    pub(crate) capture_acquire_timeout_ms: u64,

    #[serde(rename = "db_pool_capture_statement_timeout_ms")]
    pub(crate) capture_statement_timeout_ms: u64,

    #[serde(rename = "db_pool_interactive_size")]
    pub(crate) interactive_size: u32,

    #[serde(rename = "db_pool_interactive_acquire_timeout_ms")]
    pub(crate) interactive_acquire_timeout_ms: u64,

    #[serde(rename = "db_pool_interactive_statement_timeout_ms")]
    pub(crate) interactive_statement_timeout_ms: u64,

    #[serde(rename = "db_pool_background_size")]
    pub(crate) background_size: u32,

    #[serde(rename = "db_pool_background_acquire_timeout_ms")]
    pub(crate) background_acquire_timeout_ms: u64,

    #[serde(rename = "db_pool_background_statement_timeout_ms")]
    pub(crate) background_statement_timeout_ms: u64,
}

impl Default for DbPoolConfig {
    fn default() -> Self {
        Self {
            capture_size: DEFAULT_CAPTURE_LIMITS.max_connections,
            capture_acquire_timeout_ms: DEFAULT_CAPTURE_LIMITS.acquire_timeout_ms,
            capture_statement_timeout_ms: DEFAULT_CAPTURE_LIMITS.statement_timeout_ms,
            interactive_size: DEFAULT_INTERACTIVE_LIMITS.max_connections,
            interactive_acquire_timeout_ms: DEFAULT_INTERACTIVE_LIMITS.acquire_timeout_ms,
            interactive_statement_timeout_ms: DEFAULT_INTERACTIVE_LIMITS.statement_timeout_ms,
            background_size: DEFAULT_BACKGROUND_LIMITS.max_connections,
            background_acquire_timeout_ms: DEFAULT_BACKGROUND_LIMITS.acquire_timeout_ms,
            background_statement_timeout_ms: DEFAULT_BACKGROUND_LIMITS.statement_timeout_ms,
        }
    }
}

impl DbPoolConfig {
    pub(crate) fn classes(&self) -> PoolClassesConfig {
        PoolClassesConfig {
            capture: PoolClassLimits {
                max_connections: self.capture_size,
                acquire_timeout_ms: self.capture_acquire_timeout_ms,
                statement_timeout_ms: self.capture_statement_timeout_ms,
            },
            interactive: PoolClassLimits {
                max_connections: self.interactive_size,
                acquire_timeout_ms: self.interactive_acquire_timeout_ms,
                statement_timeout_ms: self.interactive_statement_timeout_ms,
            },
            background: PoolClassLimits {
                max_connections: self.background_size,
                acquire_timeout_ms: self.background_acquire_timeout_ms,
                statement_timeout_ms: self.background_statement_timeout_ms,
            },
        }
    }
}

#[cfg(test)]
#[path = "db_pool_tests.rs"]
mod db_pool_tests;
