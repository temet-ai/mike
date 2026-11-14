use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde_json::Value;

use super::{AnswerEngine, AnswerRequest, EngineError};

pub(crate) const NOT_AVAILABLE: &str = "Wright's engineer run is not in this build yet: answers need investigate::start_run with RunMode::Answer (temet-ai/mike#73)";

#[derive(Debug, Default)]
pub(crate) struct EngineerAnswers {
    in_flight: AtomicBool,
}

struct InFlight<'a>(&'a AtomicBool);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl EngineerAnswers {
    fn claim(&self) -> Result<InFlight<'_>, EngineError> {
        self.in_flight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| InFlight(&self.in_flight))
            .map_err(|_| {
                EngineError::Busy("another answer run is in flight in this process".to_owned())
            })
    }
}

#[async_trait]
impl AnswerEngine for EngineerAnswers {
    async fn answer(&self, request: AnswerRequest) -> Result<Value, EngineError> {
        let _running = self.claim()?;
        tracing::info!(
            job = request.job_id,
            message = request.message_id,
            inputs = request.inputs.len(),
            "wright: an answer run was asked for, but the engineer run is not in this build"
        );
        Err(EngineError::NotAvailable(NOT_AVAILABLE.to_owned()))
    }
}
