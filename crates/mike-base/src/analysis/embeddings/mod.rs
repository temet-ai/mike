use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::deserialize_permissive_bool;

pub mod chunker;
#[cfg(test)]
pub mod chunker_tests;
pub mod prefixes;
#[cfg(test)]
pub mod prefixes_tests;
pub mod service;
#[cfg(test)]
pub mod service_tests;

pub use chunker::chunk_text;
pub use prefixes::{ScoreFloor, prefixes_for, score_floor_for};
pub use service::EmbeddingService;

pub const DEFAULT_MODEL: &str = "embeddinggemma";

pub const DEFAULT_DIM: usize = 768;

pub const DEFAULT_CHUNK_MAX_CHARS: usize = 1500;

const DEFAULT_INFINITY_URL: &str = "http://127.0.0.1:7997";

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub enabled: bool,

    pub url: String,

    pub model: String,

    #[serde(default = "default_chunk_max_chars")]
    pub chunk_max_chars: usize,
}

fn default_chunk_max_chars() -> usize {
    DEFAULT_CHUNK_MAX_CHARS
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: DEFAULT_INFINITY_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            chunk_max_chars: DEFAULT_CHUNK_MAX_CHARS,
        }
    }
}

#[derive(Debug, Error, Clone)]
pub enum EmbedError {
    #[error("embedding service is disabled by configuration")]
    Disabled,

    #[error("embedding refused before any request: {0}")]
    Refused(String),

    #[error("Infinity HTTP error: {0}")]
    Http(String),

    #[error("Infinity response parse error: {0}")]
    Parse(String),

    #[error("Infinity returned {got} vectors for {expected} inputs")]
    LengthMismatch { got: usize, expected: usize },
}
