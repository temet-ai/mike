pub mod connection;
pub mod envelope;
pub mod reconcile;

pub use connection::{FrameRoute, IngestConnection, IngestFrame, IngestOutcome};
pub use reconcile::{ReconcileReport, Reconciler};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestSource {
    pub codex_version: String,
}

#[cfg(test)]
pub mod ingest_tests;
#[cfg(test)]
pub mod reconcile_tests;
#[cfg(test)]
pub mod transcript;
