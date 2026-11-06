use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowAvailability {
    Eligible {
        capture_version: u16,
        admitted_at: String,
        producer: String,
    },
    UnsupportedVersion {
        capture_version: u16,
    },
    NoAdmittedStart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowComponent {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowActorKind {
    Session,
    Worker,
    UnresolvedDispatch,
    Sidecar,
    Peer,
    Upstream,
    ToolService,
    Mike,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowProvenance {
    ProducerHeader,
    ExplicitCorrelation,
    SourceRow,
    Heuristic,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowTimeSemantics {
    CaptureInterval,
    RecordedObservation,
    StoredOutput,
    MeasuredLatency,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowLifecycle {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowOutcome {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowRelationshipKind {
    DispatchTool,
    ResultCarrier,
    MessageSender,
    MessageRecipient,
    MessageReply,
    ExplicitParent,
    RequestTool,
    ActivityRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionFlowRetention {
    Retained,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowSource {
    pub(crate) table: String,
    pub(crate) row_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) request_id: Option<String>,
    pub(crate) detail_href: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowActor {
    pub(crate) id: String,
    pub(crate) kind: SessionFlowActorKind,
    pub(crate) session_id: Option<String>,
    pub(crate) label: String,
    pub(crate) producer_id: Option<String>,
    pub(crate) provenance: SessionFlowProvenance,
    pub(crate) parent_actor_id: Option<String>,
    pub(crate) source: Option<SessionFlowSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowMetadata {
    pub(crate) tool_name: Option<String>,
    pub(crate) wire_tool_use_id: Option<String>,
    pub(crate) status_code: Option<u16>,
    pub(crate) model: Option<String>,
    pub(crate) latency_ms: Option<f64>,
    pub(crate) input_tokens: Option<String>,
    pub(crate) output_tokens: Option<String>,
    pub(crate) stage: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) steerer_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) scope: Option<String>,
    pub(crate) verdict: Option<String>,
    pub(crate) has_verdict: Option<bool>,
    pub(crate) attempt: Option<u32>,
    pub(crate) body_available: Option<bool>,
    pub(crate) heuristic_instance_id: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) dispatch_description: Option<String>,
    pub(crate) firing_rule_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowActivity {
    pub(crate) id: String,
    pub(crate) phase: String,
    pub(crate) actor_id: String,
    pub(crate) source: SessionFlowSource,
    pub(crate) kind: SessionFlowComponent,
    pub(crate) occurred_at: Option<String>,
    pub(crate) recorded_at: String,
    pub(crate) ended_at: Option<String>,
    pub(crate) time_semantics: SessionFlowTimeSemantics,
    pub(crate) lifecycle: SessionFlowLifecycle,
    pub(crate) outcome: Option<SessionFlowOutcome>,
    pub(crate) source_revision: Option<String>,
    pub(crate) metadata: SessionFlowMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowRelationship {
    pub(crate) id: String,
    pub(crate) from_id: Option<String>,
    pub(crate) to_id: Option<String>,
    pub(crate) kind: SessionFlowRelationshipKind,
    pub(crate) source: SessionFlowSource,
    pub(crate) phase: String,
    pub(crate) occurred_at: Option<String>,
    pub(crate) recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowCursor {
    pub(crate) version: u16,
    pub(crate) session_id: String,
    pub(crate) component: SessionFlowComponent,
    pub(crate) filter: String,
    pub(crate) after_id: String,
    pub(crate) upper_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowCoverage {
    pub(crate) component: SessionFlowComponent,
    pub(crate) lower_id: Option<String>,
    pub(crate) upper_id: Option<String>,
    pub(crate) returned_through_id: Option<String>,
    pub(crate) exhausted: bool,
    pub(crate) retention: SessionFlowRetention,
    pub(crate) capture_complete: Option<bool>,
    pub(crate) latest_only: bool,
    pub(crate) best_effort: bool,
    pub(crate) omitted_components: Vec<SessionFlowComponent>,
    pub(crate) server_observed_at: String,
    pub(crate) stale_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFlowPage {
    pub(crate) version: u16,
    pub(crate) session_id: String,
    pub(crate) availability: SessionFlowAvailability,
    pub(crate) actors: Vec<SessionFlowActor>,
    pub(crate) activities: Vec<SessionFlowActivity>,
    pub(crate) relationships: Vec<SessionFlowRelationship>,
    pub(crate) coverage: SessionFlowCoverage,
    pub(crate) next_cursor: Option<String>,
}
