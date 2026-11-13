pub mod admission;
pub mod children;
pub mod decode_items;
pub mod decode_lifecycle;
pub mod items;
pub mod observe;
pub mod project;
pub mod protocol;
pub mod usage;

pub use admission::{ExclusionReason, IntakeContext, TrafficClass};
pub use children::{
    ChildEdge, ChildGraph, ChildState, ConcurrencyDecision, ConcurrencyPolicy, EdgeKind,
    EdgeSource, ParentLink, spend_by_root,
};
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
pub use usage::{
    BillingEstimate, EstimateAvailability, ThreadSpend, TokenBreakdown, UsageLedger,
    UsageObservation,
};

#[cfg(test)]
pub mod admission_tests;
#[cfg(test)]
pub mod children_tests;
#[cfg(test)]
pub mod item_tests;
#[cfg(test)]
pub mod observe_tests;
#[cfg(test)]
pub mod projection_tests;
#[cfg(test)]
pub mod test_support;
#[cfg(test)]
pub mod usage_tests;

pub mod history;
pub mod ingest;
