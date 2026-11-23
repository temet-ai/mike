use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::members::RosterMember;
use crate::store::persistence::runtime::SessionObservationRow;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone)]
pub(super) struct ChannelState {
    pub(super) id: i64,
    pub(super) last_heartbeat_at: Option<DateTime<Utc>>,
    pub(super) last_heartbeat_interval_seconds: Option<i32>,
    pub(super) last_idle_seconds: Option<i32>,
    pub(super) worker_status: Option<String>,
    pub(super) worker_status_at: Option<DateTime<Utc>>,
    pub(super) stream_open: bool,
    pub(super) stream_opened_at: Option<DateTime<Utc>>,
    pub(super) last_delivery_at: Option<DateTime<Utc>>,
    pub(super) post_turn_summary: Option<serde_json::Value>,
    pub(super) post_turn_summary_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub(super) struct MemberState {
    pub(super) lease_expires_at: DateTime<Utc>,
    pub(super) goal: Option<String>,
    pub(super) next_steps: Option<String>,
    pub(super) display_name: Option<String>,
    pub(super) role: Option<String>,
    pub(super) instructions: Option<String>,
    pub(super) newest_request_number: Option<i64>,
    pub(super) machine_uuid: Option<Uuid>,
    pub(super) machine_name: Option<String>,
    pub(super) observation: SessionObservationRow,
    pub(super) channel: Option<ChannelState>,
}

struct StateRow {
    session_id: i64,
    lease_expires_at: DateTime<Utc>,
    goal: Option<String>,
    next_steps: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    instructions: Option<String>,
    newest_request_number: Option<i64>,
    machine_uuid: Option<Uuid>,
    machine_name: Option<String>,
    session_uuid: Uuid,
    runtime: String,
    source: String,
    source_version: Option<String>,
    fidelity: String,
    completeness: String,
    observed_at: Option<DateTime<Utc>>,
    lifecycle: Option<String>,
    lifecycle_at: Option<DateTime<Utc>>,
    control: Option<String>,
    requested_model: Option<String>,
    observed_model: Option<String>,
    usage_scope: Option<String>,
    usage_basis: Option<String>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    context_window: Option<i64>,
    model_call_count: i64,
    channel_id: Option<i64>,
    last_heartbeat_at: Option<DateTime<Utc>>,
    last_heartbeat_interval_seconds: Option<i32>,
    last_idle_seconds: Option<i32>,
    worker_status: Option<String>,
    worker_status_at: Option<DateTime<Utc>>,
    stream_open: Option<bool>,
    stream_opened_at: Option<DateTime<Utc>>,
    last_delivery_at: Option<DateTime<Utc>>,
    post_turn_summary: Option<serde_json::Value>,
    post_turn_summary_at: Option<DateTime<Utc>>,
}

impl StateRow {
    fn take_channel(&mut self) -> Option<ChannelState> {
        Some(ChannelState {
            id: self.channel_id?,
            last_heartbeat_at: self.last_heartbeat_at,
            last_heartbeat_interval_seconds: self.last_heartbeat_interval_seconds,
            last_idle_seconds: self.last_idle_seconds,
            worker_status: self.worker_status.take(),
            worker_status_at: self.worker_status_at,
            stream_open: self.stream_open.unwrap_or(false),
            stream_opened_at: self.stream_opened_at,
            last_delivery_at: self.last_delivery_at,
            post_turn_summary: self.post_turn_summary.take(),
            post_turn_summary_at: self.post_turn_summary_at,
        })
    }

    fn into_state(mut self) -> (i64, MemberState) {
        let channel = self.take_channel();
        let observation = SessionObservationRow {
            session_uuid: self.session_uuid,
            runtime: self.runtime,
            source: self.source,
            source_version: self.source_version,
            fidelity: self.fidelity,
            completeness: self.completeness,
            observed_at: self.observed_at,
            lifecycle: self.lifecycle,
            lifecycle_at: self.lifecycle_at,
            control: self.control,
            requested_model: self.requested_model,
            observed_model: self.observed_model,
            usage_scope: self.usage_scope,
            usage_basis: self.usage_basis,
            input_tokens: self.input_tokens,
            cached_input_tokens: self.cached_input_tokens,
            output_tokens: self.output_tokens,
            reasoning_output_tokens: self.reasoning_output_tokens,
            total_tokens: self.total_tokens,
            context_window: self.context_window,
            model_call_count: self.model_call_count,
        };
        let state = MemberState {
            lease_expires_at: self.lease_expires_at,
            goal: self.goal,
            next_steps: self.next_steps,
            display_name: self.display_name,
            role: self.role,
            instructions: self.instructions,
            newest_request_number: self.newest_request_number,
            machine_uuid: self.machine_uuid,
            machine_name: self.machine_name,
            observation,
            channel,
        };
        (self.session_id, state)
    }
}

impl PersistHandle {
    pub(super) async fn roster_member_states(
        &self,
        members: &[RosterMember],
    ) -> Result<HashMap<i64, MemberState>, PersistError> {
        if members.is_empty() {
            return Ok(HashMap::new());
        }
        let session_ids: Vec<i64> = members.iter().map(|m| m.session_id).collect();
        let channel_ids: Vec<i64> = members
            .iter()
            .flat_map(|m| m.channels.iter().map(|c| c.id))
            .collect();
        crate::metrics::time_db("workspace_fetch_roster_states", async move {
            let rows = sqlx::query_file_as!(
                StateRow,
                "src/store/persistence/workspace/roster/member_states.sql",
                &session_ids,
                &channel_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(StateRow::into_state).collect())
        })
        .await
    }
}
