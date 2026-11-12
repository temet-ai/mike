mod connection;
mod envelope;
mod reconcile;

pub use connection::{FrameRoute, IngestConnection, IngestFrame, IngestOutcome};
pub use reconcile::{ReconcileReport, Reconciler};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestSource {
    pub codex_version: String,
}

#[cfg(test)]
mod ingest_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
pub(crate) mod transcript;
