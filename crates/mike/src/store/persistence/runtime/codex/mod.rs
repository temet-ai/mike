mod apply;
mod codec;
mod context;
mod effects;
mod generation;
mod ingest;
#[cfg(test)]
mod ingest_db_tests;
mod intake;
mod projection_reads;
mod reads;
mod state;
mod types;

pub use context::{ClassificationProvenance, CodexIntakeContext};
pub use types::{
    CodexCaptureOutcome, CodexEffectPage, CodexGeneration, CodexItemPage,
    CodexProjectionApplication, CodexProjectionApplyOutcome, CodexProjectionEffect,
    CodexReceiptInput, CodexStoredItem, RawEventPage, StoredRuntimeReceipt,
};

use crate::store::persistence::PersistError;

const PROJECTOR_VERSION: i32 = 1;

fn invariant(message: &str) -> PersistError {
    PersistError::DataInvariant(format!("Codex persistence: {message}"))
}

#[cfg(feature = "test-seams")]
pub mod test_seams;
