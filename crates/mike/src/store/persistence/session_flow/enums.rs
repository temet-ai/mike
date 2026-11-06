use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowAvailability {
    Eligible {
        capture_version: u16,
        admitted_at: DateTime<Utc>,
        producer: String,
    },
    UnsupportedVersion {
        capture_version: u16,
    },
    NoAdmittedStart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowComponent {
    Requests,
    Dispatches,
    Tools,
    Results,
    Messages,
    WorkerStatus,
    ManifestStages,
    LessonInjections,
    VerifierEvaluations,
    Judgments,
    LlmCalls,
    Embeddings,
    Consolidation,
    Interventions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowActorKind {
    Session,
    Worker,
    UnresolvedDispatch,
    Sidecar,
    Peer,
    Upstream,
    ToolService,
    Mike,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowProvenance {
    ProducerHeader,
    ExplicitCorrelation,
    SourceRow,
    Heuristic,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowTimeSemantics {
    CaptureInterval,
    RecordedObservation,
    StoredOutput,
    MeasuredLatency,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowLifecycle {
    Started,
    Observed,
    Completed,
    Queued,
    Claimed,
    Attempted,
    Delivered,
    Consumed,
    Expired,
    Reverted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowOutcome {
    Success,
    Error,
    Applied,
    Unchanged,
    Skipped,
    FailedClosed,
    Expired,
    Reverted,
    Resolved,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowRelationshipKind {
    DispatchTool,
    ResultCarrier,
    MessageSender,
    MessageRecipient,
    MessageReply,
    ExplicitParent,
    RequestTool,
    ActivityRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowRetention {
    Retained,
    Partial,
    Unknown,
}
