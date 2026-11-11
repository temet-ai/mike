mod admission;
mod decode_items;
mod decode_lifecycle;
mod items;
mod observe;
mod project;
mod protocol;
mod usage;

pub use admission::{ExclusionReason, IntakeContext, TrafficClass};
pub use items::{
    CommandFacts, ItemCategory, ItemKey, ItemObservation, ItemReduction, ItemSnapshot, ItemUpdate,
    ProjectedItem, ReceiptApplication, ReductionStatus, SnapshotKind, TextChannel, ToolFacts,
    reduce_item,
};
pub use observe::{IntakeResult, observe};
pub use project::{
    LifecycleObservation, ProjectionBatch, ProjectionEffect, ProjectionStatus, SignalKind,
    SignalObservation, ThreadFacts, decode_observation,
};
pub use protocol::{
    Evidence, EvidenceOrigin, Field, IssueKind, ProjectionIssue, ReceiptId, RequestId, RpcScope,
};
pub use usage::{BillingEstimate, EstimateAvailability, TokenBreakdown, UsageObservation};

#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod item_tests;
#[cfg(test)]
mod observe_tests;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod usage_tests;

pub mod history;
pub mod ingest;
