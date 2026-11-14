use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::store::persistence::PersistHandle;

pub(crate) mod answer;
pub(crate) mod engineer;
pub(crate) mod job;
pub(crate) mod notice;
pub mod pair;
pub(crate) mod steer;
pub(crate) mod steer_apply;
mod steer_owners;
pub(crate) mod steering;
pub(crate) mod token;
pub(crate) mod worker;

#[cfg(test)]
mod fakes_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod post_tests;
#[cfg(test)]
mod steer_tests;
#[cfg(test)]
mod token_tests;
#[cfg(test)]
mod undo_tests;

pub use worker::spawn_conversation_worker;

pub(crate) use steer::Change;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum EngineError {
    #[error("{0}")]
    NotAvailable(String),
    #[error("{0}")]
    Busy(String),
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnswerRequest {
    pub(crate) job_id: i64,
    pub(crate) message_id: i64,
    pub(crate) brief: String,
    pub(crate) inputs: Vec<(String, Value)>,
}

#[async_trait]
pub(crate) trait AnswerEngine: Send + Sync {
    async fn answer(&self, request: AnswerRequest) -> Result<Value, EngineError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SteerError {
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Failed(String),
}

#[async_trait]
pub(crate) trait SteerApply: Send + Sync {
    async fn read(&self, change: &Change) -> Result<Value, SteerError>;
    async fn apply(&self, steer_id: i64, change: &Change) -> Result<Value, SteerError>;
    async fn undo(
        &self,
        change: &Change,
        prior: &Value,
        effect: &Value,
    ) -> Result<Value, SteerError>;
}

#[derive(Clone)]
pub(crate) struct Conversation {
    pub(crate) persist: PersistHandle,
    pub(crate) engine: Arc<dyn AnswerEngine>,
    pub(crate) steers: Arc<dyn SteerApply>,
}
