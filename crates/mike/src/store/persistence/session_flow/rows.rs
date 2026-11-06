use super::enums::*;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowRowId {
    Bigint(i64),
    Uuid(Uuid),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowSource {
    pub table: String,
    pub row_id: FlowRowId,
    pub session_id: Option<Uuid>,
    pub request_id: Option<i64>,
    pub detail_href: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowActor {
    pub id: String,
    pub kind: FlowActorKind,
    pub session_id: Option<Uuid>,
    pub label: String,
    pub producer_id: Option<String>,
    pub provenance: FlowProvenance,
    pub parent_actor_id: Option<String>,
    pub source: Option<FlowSource>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FlowMetadata {
    pub expires_at: Option<DateTime<Utc>>,
    pub source_uuid: Option<Uuid>,
    pub lesson_id: Option<i64>,
    pub lesson_uuid: Option<Uuid>,
    pub intervention_uuid: Option<Uuid>,
    pub run_id: Option<i64>,
    pub quality_score: Option<f64>,
    pub rubric_version: Option<u32>,
    pub persona: Option<String>,
    pub schema_version: Option<u32>,
    pub tool_name: Option<String>,
    pub ordinal: Option<u32>,
    pub thread_id: Option<i64>,
    pub wire_tool_use_id: Option<String>,
    pub status_code: Option<u16>,
    pub model: Option<String>,
    pub latency_ms: Option<f64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub stage: Option<String>,
    pub role: Option<String>,
    pub steerer_id: Option<i64>,
    pub turn_id: Option<i64>,
    pub cost_usd: Option<f64>,
    pub scope: Option<String>,
    pub verdict: Option<String>,
    pub has_verdict: Option<bool>,
    pub attempt: Option<u32>,
    pub body_available: Option<bool>,
    pub heuristic_instance_id: Option<i64>,
    pub agent_name: Option<String>,
    pub dispatch_description: Option<String>,
    pub firing_rule_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowActivity {
    pub id: String,
    pub phase: String,
    pub actor_id: String,
    pub source: FlowSource,
    pub kind: FlowComponent,
    pub occurred_at: Option<DateTime<Utc>>,
    pub recorded_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub time_semantics: FlowTimeSemantics,
    pub lifecycle: FlowLifecycle,
    pub outcome: Option<FlowOutcome>,
    pub source_revision: Option<String>,
    pub metadata: FlowMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowRelationship {
    pub id: String,
    pub from_id: Option<String>,
    pub to_id: Option<String>,
    pub kind: FlowRelationshipKind,
    pub source: FlowSource,
    pub phase: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowCursor {
    pub version: u16,
    pub session_id: Uuid,
    pub component: FlowComponent,
    pub filter: String,
    pub after_id: i64,
    pub upper_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowCoverage {
    pub component: FlowComponent,
    pub lower_id: Option<FlowRowId>,
    pub upper_id: Option<FlowRowId>,
    pub returned_through_id: Option<FlowRowId>,
    pub exhausted: bool,
    pub retention: FlowRetention,
    pub capture_complete: Option<bool>,
    pub latest_only: bool,
    pub best_effort: bool,
    pub omitted_components: Vec<FlowComponent>,
    pub server_observed_at: DateTime<Utc>,
    pub stale_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowPage {
    pub version: u16,
    pub session_id: Uuid,
    pub availability: FlowAvailability,
    pub actors: Vec<FlowActor>,
    pub activities: Vec<FlowActivity>,
    pub relationships: Vec<FlowRelationship>,
    pub coverage: FlowCoverage,
    pub next_cursor: Option<FlowCursor>,
}
