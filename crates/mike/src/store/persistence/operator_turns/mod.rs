use chrono::{DateTime, Utc};

use crate::types::SortOrder;

mod chunks;
pub mod correction;
pub(crate) mod eligibility;
mod embedding;
pub(crate) mod judge_claim;
mod judgment;
mod listing;
mod live_claim;
mod outcomes;
pub mod projection;
pub mod projection_reads;
mod reads;
mod unjudged;

pub(crate) use embedding::vector_to_text_literal;
pub use live_claim::{BACKLOG_COUNT_CAP, LiveBacklog, LiveClaim, LiveFailure};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OperatorTurnRow {
    pub(crate) turn_id: i64,
    pub(crate) session_id: String,
    pub(crate) project_id: i64,
    pub start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) start_request_number: i64,
    pub(crate) end_request_number: i64,
    pub(crate) user_prompt: String,
    pub(crate) final_assistant_text: String,
    pub(crate) tool_call_count: i32,
    pub(crate) tool_error_count: i32,
    pub(crate) files_touched: Vec<String>,
    pub(crate) tools_used: Vec<String>,
    pub(crate) agents_dispatched: Vec<String>,
    pub(crate) skills_used: Vec<String>,
    pub(crate) weasel_findings_count: i32,
    pub(crate) total_input_tokens: i64,
    pub(crate) total_output_tokens: i64,
    pub(crate) duration_ms: i64,
    pub(crate) stop_response: Option<String>,
    pub(crate) stop_reason: Option<String>,
    pub(crate) stop_request_id: Option<i64>,
    pub(crate) interrupted: bool,
    pub(crate) outcome: String,
    pub(crate) outcome_evidence: Option<String>,
    pub(crate) classified_at: Option<DateTime<Utc>>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) prompt_kind: Option<String>,
    pub(crate) subject_command: Option<String>,
    pub(crate) subject_agent: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) outcome_bundle_id: Option<i64>,
    pub(crate) reaction: Option<String>,
    pub(crate) delivery: Option<String>,
    pub(crate) outcome_source: Option<String>,
    pub(crate) counts_as_correction: bool,
    pub(crate) live_verdict_id: Option<i64>,
    pub(crate) outcome_projection_version: Option<i16>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QuiescentTurnRow {
    pub session_uuid: uuid::Uuid,
    pub session_id: i64,
    pub turn_id: i64,
    pub end_request_id: i64,
    pub steerer_id: i64,
    pub idle_seconds: i64,
    pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuiescenceTarget {
    pub steerer_id: i64,
    pub idle_after_secs: i64,
    pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TriggerTurn {
    pub session_db_id: i64,
    pub operator_turn_id: Option<i64>,
    pub claude_code_agent_id: Option<String>,
    pub agent_instance_id: Option<i64>,
    pub subagent_persona: Option<String>,
}

impl TriggerTurn {
    pub fn is_agent(&self) -> bool {
        self.claude_code_agent_id.is_some()
            || self.agent_instance_id.is_some()
            || self.subagent_persona.is_some()
    }
}

pub(crate) const TURN_SELECT_BASE: &str = "
    SELECT
        t.id AS turn_id, s.session_uuid::text AS session_id, t.project_id,
        t.start_request_id, t.end_request_id,
        COALESCE(num.start_request_number, 0) AS start_request_number,
        COALESCE(num.end_request_number,   0) AS end_request_number,
        COALESCE(tx.user_prompt, '')          AS user_prompt,
        COALESCE(tx.final_assistant_text, '') AS final_assistant_text,
        COALESCE(agg.tool_call_count, 0)::int      AS tool_call_count,
        COALESCE(agg.tool_error_count, 0)::int     AS tool_error_count,
        COALESCE(agg.files_touched,     '{}'::text[]) AS files_touched,
        COALESCE(agg.tools_used,        '{}'::text[]) AS tools_used,
        COALESCE(agg.agents_dispatched, '{}'::text[]) AS agents_dispatched,
        COALESCE(agg.skills_used,       '{}'::text[]) AS skills_used,
        COALESCE(agg.weasel_findings_count, 0)::int     AS weasel_findings_count,
        COALESCE(agg.total_input_tokens, 0)::bigint     AS total_input_tokens,
        COALESCE(agg.total_output_tokens, 0)::bigint    AS total_output_tokens,
        COALESCE(EXTRACT(EPOCH FROM (er.completed_at - sr.created_at))::bigint * 1000, 0) AS duration_ms,
        stp_text.stop_response,
        stp.stop_reason,
        stp.stop_request_id,
        (COALESCE(tf.interrupted, false) OR COALESCE(stp.unsettled, false)) AS interrupted,
        toc.code AS outcome,
        ev.last_user_message AS outcome_evidence,
        t.classified_at,
        t.created_at,
        COALESCE(t.classified_at, t.created_at) AS updated_at,
        pk_kinds.code AS prompt_kind,
        t.subject_command,
        t.subject_agent,
        sr.model AS model,
        t.outcome_bundle_id,
        t.reaction,
        t.delivery,
        t.outcome_source,
        (toc.code = 'corrected' OR (toc.code = 'abandoned' AND t.outcome_source IS NOT DISTINCT FROM 'frustration_close')) AS counts_as_correction,
        t.live_verdict_id,
        t.outcome_projection_version
      FROM memory.operator_turns t
      JOIN sessions.sessions s ON s.id = t.session_id
      JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
      LEFT JOIN memory.turn_facts tf ON tf.operator_turn_id = t.id
      LEFT JOIN mike.operator_turn_aggregates_v agg ON agg.turn_id = t.id
      LEFT JOIN mike.operator_turn_text_v       tx  ON tx.turn_id  = t.id
      LEFT JOIN requests.requests                   ev
             ON ev.id = t.outcome_evidence_request_id
      LEFT JOIN requests.requests                   sr
             ON sr.id = t.start_request_id
      LEFT JOIN requests.requests                   er
             ON er.id = t.end_request_id AND er.completed_at IS NOT NULL
      LEFT JOIN prompt_kinds.classifications pk_cls
             ON pk_cls.request_id = t.start_request_id
      LEFT JOIN prompt_kinds.kinds pk_kinds
             ON pk_kinds.id = pk_cls.kind_id
      LEFT JOIN LATERAL (
            SELECT COUNT(*) FILTER (WHERE (r_num.created_at, r_num.id) <= (sr.created_at, sr.id)) AS start_request_number,
                   COUNT(*) FILTER (WHERE (r_num.created_at, r_num.id) <= (er.created_at, er.id)) AS end_request_number
            FROM requests.requests r_num
            WHERE r_num.session_id = t.session_id
      ) num ON true
      LEFT JOIN LATERAL (
            SELECT MIN(t2.start_request_id) AS next_start
              FROM memory.operator_turns t2
             WHERE t2.session_id = t.session_id
               AND t2.start_request_id > t.start_request_id
      ) nxt ON true
      LEFT JOIN LATERAL (
            SELECT r.id AS stop_request_id,
                   (resp.id IS NULL) AS unsettled,
                   CASE WHEN resp.id IS NULL THEN 'aborted' ELSE rsr.code END AS stop_reason
              FROM requests.requests r
              JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
              LEFT JOIN responses.responses    resp ON resp.request_id = r.id
              LEFT JOIN responses.stop_reasons rsr  ON rsr.id = resp.stop_reason_id
             WHERE r.session_id = t.session_id
               AND r.id >= t.start_request_id
               AND (nxt.next_start IS NULL OR r.id < nxt.next_start)
               AND ro.code IN ('primary', 'primary_first')
               AND EXISTS (
                   SELECT 1 FROM responses.stream_blocks b2
                   JOIN responses.stream_block_types bt2 ON bt2.id = b2.block_type_id
                   WHERE b2.request_id = r.id AND bt2.code = 'text')
             ORDER BY r.id DESC
             LIMIT 1
      ) stp ON true
      -- Keyed on the already-resolved stp.stop_request_id, not correlated per candidate row (sql-set-based.md).
      LEFT JOIN LATERAL (
            SELECT string_agg(b.content, '' ORDER BY b.seq_ord) AS stop_response
              FROM responses.stream_blocks b
              JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
             WHERE b.request_id = stp.stop_request_id AND bt.code = 'text'
      ) stp_text ON stp.stop_request_id IS NOT NULL
";

#[derive(Debug, Clone, Default)]
pub(crate) struct ListTurnsFilter {
    pub(crate) project_path: Option<String>,
    pub(crate) session_db_id: Option<i64>,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) until: Option<DateTime<Utc>>,
    pub(crate) outcome: Option<String>,
    pub(crate) correction: bool,
    pub(crate) subject_command: Option<String>,
    pub(crate) subject_agent: Option<String>,
    pub(crate) skill: Option<String>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) order: SortOrder,
}

#[derive(Debug, Clone, Default)]
pub struct SessionOverviewFilter {
    pub prompt_kind: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub omit_stop_response: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JudgeCandidateRow {
    pub turn_id: i64,
    pub user_prompt: String,
    pub final_assistant_text: String,
    pub tool_call_count: i32,
    pub tool_error_count: i32,
}

#[derive(Debug, Clone)]
pub struct TurnJudgmentStamp<'a> {
    pub turn_id: i64,
    pub outcome_code: &'a str,
    pub judge_model: &'a str,
    pub rubric_version: &'a str,
    pub detail: Option<TurnJudgeDetail>,
    pub bundle_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TurnJudgeDetail {
    pub rationale: Option<String>,
    pub unmet_ask: Option<String>,
    pub evidence_quality: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct PendingEmbeddingRow {
    pub(crate) turn_id: i64,
    pub(crate) user_prompt: String,
    pub(crate) final_assistant_text: String,
}

pub struct PendingChunk {
    pub chunk_idx: i32,
    pub chunk_role: &'static str,
    pub chunk_text: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SessionOverviewTurnRow {
    pub(crate) turn_id: i64,
    pub(crate) user_prompt: String,
    pub(crate) stop_response: Option<String>,
    pub(crate) stop_reason: Option<String>,
    pub(crate) stop_request_id: Option<i64>,
    pub(crate) interrupted: bool,
    pub(crate) outcome: String,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) start_request_number: i64,
    pub(crate) end_request_number: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) closed_at: Option<DateTime<Utc>>,
    pub(crate) prompt_kind: Option<String>,
    pub(crate) subject_command: Option<String>,
    pub(crate) subject_agent: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) reaction: Option<String>,
    pub(crate) delivery: Option<String>,
    pub(crate) outcome_source: Option<String>,
    pub(crate) live_verdict_id: Option<i64>,
    pub(crate) outcome_projection_version: Option<i16>,
}

const SESSION_OVERVIEW_SELECT_BASE: &str = "
    SELECT
        t.id AS turn_id,
        COALESCE(t.user_prompt, sr.human_prompt, '') AS user_prompt,
        stp_text.stop_response,
        stp.stop_reason,
        stp.stop_request_id,
        (COALESCE(tf.interrupted, false) OR COALESCE(stp.unsettled, false)) AS interrupted,
        toc.code AS outcome,
        t.start_request_id,
        t.end_request_id,
        COALESCE(num.start_request_number, 0) AS start_request_number,
        COALESCE(num.end_request_number,   0) AS end_request_number,
        t.created_at,
        t.closed_at,
        pk_kinds.code AS prompt_kind,
        t.subject_command,
        t.subject_agent,
        sr.model AS model,
        t.reaction,
        t.delivery,
        t.outcome_source,
        t.live_verdict_id,
        t.outcome_projection_version
      FROM memory.operator_turns t
      JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
      LEFT JOIN memory.turn_facts tf ON tf.operator_turn_id = t.id
      LEFT JOIN requests.requests sr
             ON sr.id = t.start_request_id
      LEFT JOIN requests.requests er
             ON er.id = t.end_request_id AND er.completed_at IS NOT NULL
      LEFT JOIN prompt_kinds.classifications pk_cls
             ON pk_cls.request_id = t.start_request_id
      LEFT JOIN prompt_kinds.kinds pk_kinds
             ON pk_kinds.id = pk_cls.kind_id
      LEFT JOIN LATERAL (
            SELECT COUNT(*) FILTER (WHERE (r_num.created_at, r_num.id) <= (sr.created_at, sr.id)) AS start_request_number,
                   COUNT(*) FILTER (WHERE (r_num.created_at, r_num.id) <= (er.created_at, er.id)) AS end_request_number
            FROM requests.requests r_num
            WHERE r_num.session_id = t.session_id
      ) num ON true
      LEFT JOIN LATERAL (
            SELECT MIN(t2.start_request_id) AS next_start
              FROM memory.operator_turns t2
             WHERE t2.session_id = t.session_id
               AND t2.start_request_id > t.start_request_id
      ) nxt ON true
      LEFT JOIN LATERAL (
            SELECT r.id AS stop_request_id,
                   (resp.id IS NULL) AS unsettled,
                   CASE WHEN resp.id IS NULL THEN 'aborted' ELSE rsr.code END AS stop_reason
              FROM requests.requests r
              JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
              LEFT JOIN responses.responses    resp ON resp.request_id = r.id
              LEFT JOIN responses.stop_reasons rsr  ON rsr.id = resp.stop_reason_id
             WHERE r.session_id = t.session_id
               AND r.id >= t.start_request_id
               AND (nxt.next_start IS NULL OR r.id < nxt.next_start)
               AND ro.code IN ('primary', 'primary_first')
               AND EXISTS (
                   SELECT 1 FROM responses.stream_blocks b2
                   JOIN responses.stream_block_types bt2 ON bt2.id = b2.block_type_id
                   WHERE b2.request_id = r.id AND bt2.code = 'text')
             ORDER BY r.id DESC
             LIMIT 1
      ) stp ON true
      -- Keyed on the already-resolved stp.stop_request_id, not correlated per candidate row (sql-set-based.md).
      LEFT JOIN LATERAL (
            SELECT string_agg(b.content, '' ORDER BY b.seq_ord) AS stop_response
              FROM responses.stream_blocks b
              JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
             WHERE b.request_id = stp.stop_request_id AND bt.code = 'text'
      ) stp_text ON stp.stop_request_id IS NOT NULL
";

const LIST_FOR_SESSION_LIMIT: i64 = 1000;
