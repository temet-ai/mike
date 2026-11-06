mod audit;
mod control;
mod entries;
mod page;
mod request;

pub use control::{
    HistoryAction, HistoryBlocker, HistoryCoverage, HistoryCursorLedger, HistoryCursorVisit,
    HistoryStep, next_history_step,
};
pub use entries::{HistoryEntry, HistoryItem, HistoryMetadata, HistoryTurn};
pub use page::HistoryPage;
pub use request::{HistoryIdentity, HistoryQuery, HistoryRequest, HistoryScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryErrorKind {
    InvalidIdentity,
    ZeroPageLimit,
    InvalidObservation,
    InvalidEnvelope,
    UncorrelatedResponse,
    ConflictingScope,
    DuplicateControlField,
    RpcError,
    InvalidData,
    InvalidIdentifier,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind:?} at {json_pointer}")]
pub struct HistoryError {
    pub kind: HistoryErrorKind,
    pub json_pointer: String,
}

impl HistoryError {
    pub(super) fn new(kind: HistoryErrorKind, pointer: impl Into<String>) -> Self {
        Self {
            kind,
            json_pointer: pointer.into(),
        }
    }
}
