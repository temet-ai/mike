use serde::{Deserialize, Serialize};

use super::request_id::RequestId;
use crate::types::TokenUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowChangedTable {
    Requests,
    Sessions,
    FilterRules,
    PromptCompositions,
    Interventions,
    MailboxMessages,
    MailboxThreads,
    LessonInjections,
    SteeringEvaluations,
    ConsolidationActions,
    ConsolidationRuns,
    SessionJudgesVerdicts,
    LlmCalls,
    SessionEmbeddings,
    LaunchLaunches,
    LaunchFleets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RowChangedOp {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReason {
    ListenerEstablishment,
    Recovery,
    UnusableNotification,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemoryTickPayload {
    pub entry_count: u64,
    pub request_body_bytes: u64,
    pub response_body_bytes: u64,
    pub filtered_request_body_bytes: u64,
    pub tool_calls_bytes: u64,
    pub tool_results_bytes: u64,
    pub tool_results_in_body_bytes: u64,
    pub system_prompt_blocks_bytes: u64,
    pub entries_total_bytes: u64,

    pub session_index_keys: u64,
    pub composable_session_index_keys: u64,
    pub session_meta_keys: u64,
    pub in_flight_count: u64,

    pub process_rss_bytes: Option<u64>,
    pub fastembed_model_bytes: u64,

    pub max_entries: u64,
    pub max_store_bytes: u64,
    pub over_byte_budget: bool,

    pub broadcast_receivers: usize,
    pub broadcast_backlog: usize,
    pub db_pool_size: Option<u32>,
    pub db_pool_idle: Option<usize>,
    pub tick_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ProxyEvent {
    ModelConfirmed {
        id: RequestId,
        model: String,
    },
    UpstreamMessageIdReceived {
        id: RequestId,
        upstream_message_id: String,
    },
    TextDelta {
        id: RequestId,
        text: String,
    },
    ThinkingDelta {
        id: RequestId,
        thinking: String,
    },
    SignatureDelta {
        id: RequestId,
        signature: String,
    },
    ToolUseStart {
        id: RequestId,
        tool_id: String,
        tool_name: String,
    },
    ToolInputDelta {
        id: RequestId,
        partial_json: String,
    },
    ToolUseEnd {
        id: RequestId,
    },
    ResponseComplete {
        id: RequestId,
        stop_reason: Option<String>,
        stop_details: Option<serde_json::Value>,
        usage: Option<TokenUsage>,
    },
    ResponseError {
        id: RequestId,
        error: String,
    },
    UsageReceived {
        id: RequestId,
        usage: TokenUsage,
    },
    MessageFieldsReceived {
        id: RequestId,
        fields: serde_json::Value,
    },
    DiagnosticReady {
        id: RequestId,
        count: usize,
        #[serde(skip)]
        accumulated_text: String,
    },
    RowChanged {
        table: RowChangedTable,
        id: String,
        session_id: Option<String>,
        op: RowChangedOp,
    },
    ResyncRequired {
        reason: ResyncReason,
    },
    MemoryShed(crate::observability::shed::MemoryShedEvent),
    MemoryTick(MemoryTickPayload),
    StreamingStarted {
        id: RequestId,
        status: u16,
        ttfb_ms: u64,
        model: Option<String>,
    },
    SteeringFired {
        intervention_id: u64,
        session_id: String,
        session_uuid: String,
        steerer_name: Option<String>,
        steerer_display_name: Option<String>,
        authority: Option<String>,
        mode: Option<String>,
        verdict: Option<String>,
        confidence: Option<f32>,
        off_thread_topic: Option<String>,
        payload_text: Option<String>,
        trigger_request_id: u64,
    },

    UserPromptReady {
        request_id: RequestId,
        session_id: String,
        #[serde(skip)]
        user_text: String,
    },

    OperatorTurnClosed {
        closed_turn_id: i64,
        session_id: String,
        new_request_id: RequestId,
        #[serde(skip)]
        human_prompt: String,
    },
}
