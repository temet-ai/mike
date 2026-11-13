pub mod apply;
pub mod codec;
pub mod context;
pub mod effects;
pub mod generation;
pub mod ingest;
#[cfg(test)]
pub mod ingest_db_tests;
pub mod intake;
pub mod projection_reads;
pub mod reads;
pub mod state;
pub mod types;

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
