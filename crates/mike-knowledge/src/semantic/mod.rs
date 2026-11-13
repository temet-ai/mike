pub mod anchors;
pub mod backtest;
pub mod builder;
pub mod check;
pub mod cluster;
pub mod config;
pub mod drift;
pub mod drift_job;
pub mod egress;
pub mod embedder;
pub mod facets;
pub mod floors;
pub mod floors_job;
pub mod knowledge_state;
pub mod neighbours;
pub mod retrieve;
pub mod sections;
pub mod tag_job;
pub mod tagger;
pub mod worker;

#[cfg(test)]
#[path = "tests/facets_tests.rs"]
pub mod facets_tests;
#[cfg(test)]
#[path = "tests/model_tests.rs"]
pub mod model_tests;
#[cfg(test)]
#[path = "tests/pure_tests.rs"]
pub mod pure_tests;

use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::analysis::embeddings::EmbedError;
use crate::store::persistence::PersistError;

pub use config::SemanticConfig;
pub use knowledge_state::KnowledgeState;
#[cfg(any(test, feature = "test-seams"))]
pub use worker::run_semantic_once;
pub use worker::spawn_semantic_worker;

pub const BUILDER_VERSION: &str = "semantic-items-v1";

#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    #[error(transparent)]
    Persist(#[from] PersistError),
    #[error("embedding call failed: {0}")]
    Embed(#[from] EmbedError),
    #[error(transparent)]
    Query(#[from] retrieve::QueryError),
    #[error("the embedding URL is not loopback and semantic.allow_remote_embedding is off")]
    EgressBlocked,
    #[error("no current embedder is registered yet")]
    NoEmbedder,
    #[error("the embedding config changed during the call: {0}")]
    SnapshotChanged(String),
    #[error("the inline embed timed out after {0} ms")]
    Timeout(u64),
    #[error("the semantic_context toggle is off")]
    Disabled,
    #[error("no semantic item with id {0}")]
    NoItem(i64),
    #[error("{0}")]
    Import(String),
}

impl SemanticError {
    pub fn is_embedding_disabled(&self) -> bool {
        matches!(self, Self::Embed(EmbedError::Disabled))
    }

    pub fn trips_breaker(&self) -> bool {
        !self.is_embedding_disabled() && matches!(self, Self::Embed(_) | Self::Persist(_))
    }

    pub fn status_code(&self) -> &'static str {
        match self {
            Self::EgressBlocked => "egress_blocked",
            Self::Timeout(_) => "embed_timeout",
            Self::Disabled => "disabled",
            Self::NoEmbedder => "no_embedder",
            _ => "error",
        }
    }
}

pub fn embed_error(e: EmbedError) -> SemanticError {
    match e {
        EmbedError::Refused(_) => SemanticError::EgressBlocked,
        other => SemanticError::Embed(other),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WorkerStatus {
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_status: Option<String>,
    pub breaker_open: bool,
    pub embedder_key: Option<String>,
    pub last_floor_attempt: Option<DateTime<Utc>>,
    pub last_daily_run: Option<DateTime<Utc>>,
    pub last_weekly_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SemanticState {
    pub config: SemanticConfig,
    pub status: WorkerStatus,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticHandle {
    inner: Arc<ArcSwap<SemanticState>>,
}

impl SemanticHandle {
    pub fn load(&self) -> Arc<SemanticState> {
        self.inner.load_full()
    }

    pub fn config(&self) -> SemanticConfig {
        self.inner.load().config.clone()
    }

    pub fn update(&self, f: impl Fn(&mut SemanticState)) {
        let mut next = (*self.inner.load_full()).clone();
        f(&mut next);
        self.inner.store(Arc::new(next));
    }
}
