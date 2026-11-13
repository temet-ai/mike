//! The operator-turn DTOs the recall handlers answer with and search events record.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OperatorTurnDto {
    pub turn_id: i64,
    pub session_id: String,
    pub project_id: i64,
    pub start_request_id: i64,
    pub end_request_id: i64,
    pub start_request_number: i64,
    pub end_request_number: i64,
    pub user_prompt: String,
    pub final_assistant_text: String,
    pub tool_call_count: i32,
    pub tool_error_count: i32,
    pub files_touched: Vec<String>,
    pub tools_used: Vec<String>,
    pub agents_dispatched: Vec<String>,
    pub skills_used: Vec<String>,
    pub weasel_findings_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub duration_ms: i64,
    pub stop_response: Option<String>,
    pub stop_reason: Option<String>,
    pub stop_request_id: Option<i64>,
    pub interrupted: bool,
    pub outcome: String,
    pub outcome_evidence: Option<String>,
    pub classified_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub outcome_bundle_id: Option<i64>,
    pub reaction: Option<String>,
    pub delivery: Option<String>,
    pub outcome_source: Option<String>,
    pub counts_as_correction: bool,
    pub live_verdict_id: Option<i64>,
    pub outcome_projection_version: Option<i16>,
    pub legacy: bool,
    pub pending: bool,
}

impl From<crate::store::operator_turns::OperatorTurnRow> for OperatorTurnDto {
    fn from(r: crate::store::operator_turns::OperatorTurnRow) -> Self {
        let (legacy, pending) =
            crate::store::operator_turns::projection_flags(r.outcome_source.as_deref(), &r.outcome);
        Self {
            turn_id: r.turn_id,
            session_id: r.session_id,
            project_id: r.project_id,
            start_request_id: r.start_request_id,
            end_request_id: r.end_request_id,
            start_request_number: r.start_request_number,
            end_request_number: r.end_request_number,
            user_prompt: r.user_prompt,
            final_assistant_text: r.final_assistant_text,
            tool_call_count: r.tool_call_count,
            tool_error_count: r.tool_error_count,
            files_touched: r.files_touched,
            tools_used: r.tools_used,
            agents_dispatched: r.agents_dispatched,
            skills_used: r.skills_used,
            weasel_findings_count: r.weasel_findings_count,
            total_input_tokens: r.total_input_tokens,
            total_output_tokens: r.total_output_tokens,
            duration_ms: r.duration_ms,
            stop_response: r.stop_response,
            stop_reason: r.stop_reason,
            stop_request_id: r.stop_request_id,
            interrupted: r.interrupted,
            outcome: r.outcome,
            outcome_evidence: r.outcome_evidence,
            classified_at: r.classified_at.map(|t| t.to_rfc3339()),
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            prompt_kind: r.prompt_kind,
            subject_command: r.subject_command,
            subject_agent: r.subject_agent,
            model: r.model,
            outcome_bundle_id: r.outcome_bundle_id,
            reaction: r.reaction,
            delivery: r.delivery,
            outcome_source: r.outcome_source,
            counts_as_correction: r.counts_as_correction,
            live_verdict_id: r.live_verdict_id,
            outcome_projection_version: r.outcome_projection_version,
            legacy,
            pending,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SimilarTurnDto {
    #[serde(flatten)]
    pub turn: OperatorTurnDto,
    pub rank: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerank_score: Option<f64>,
}
