use chrono::{DateTime, Utc};

use super::types::ContextItem;

#[derive(Debug, Clone)]
pub(crate) struct ThreadCore {
    pub(crate) id: i64,
    pub(crate) thread_uuid: uuid::Uuid,
    pub(crate) subject: String,
}

#[derive(Debug, Clone)]
pub(crate) struct MessageCore {
    pub(crate) id: i64,
    pub(crate) thread_id: Option<i64>,
    pub(crate) body: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ThreadResolution {
    None,
    Resolved {
        thread_id: i64,
        thread_uuid: uuid::Uuid,
        subject: String,
        in_reply_to_id: Option<i64>,
    },
    ThreadNotFound,
    ParentMessageNotFound,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ThreadSummaryRow {
    pub(crate) id: i64,
    pub(crate) thread_uuid: uuid::Uuid,
    pub(crate) subject: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_message_at: DateTime<Utc>,
    pub(crate) resolved_at: Option<DateTime<Utc>>,
    pub(crate) created_by_session_uuid: Option<uuid::Uuid>,
    pub(crate) participant_uuids: Vec<uuid::Uuid>,
    pub(crate) tag_codes: Vec<String>,
    pub(crate) message_count: i64,
    pub(crate) unread_count: i64,
    pub(crate) oldest_pending_queued_at: Option<DateTime<Utc>>,
    pub(crate) worst_rank: Option<i32>,
    pub(crate) project_uuid: Option<uuid::Uuid>,
    pub(crate) project_name: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) project_pinned: Option<bool>,
    pub(crate) fleet_uuid: Option<uuid::Uuid>,
    pub(crate) fleet_slug: Option<String>,
    pub(crate) fleet_state: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ParticipantIdentityRow {
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) title: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) display_name: Option<String>,
    pub(crate) agent_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ThreadSummaryFilter {
    pub(crate) thread_uuid: Option<uuid::Uuid>,
    pub(crate) participant_session_uuid: Option<uuid::Uuid>,
    pub(crate) project_ids: Option<Vec<i64>>,
    pub(crate) resolved: Option<bool>,
    pub(crate) tag: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ThreadMessageRow {
    pub(crate) message_uuid: uuid::Uuid,
    pub(crate) to_session_uuid: Option<uuid::Uuid>,
    pub(crate) from_session_uuid: Option<uuid::Uuid>,
    pub(crate) body: String,
    pub(crate) queued_at: DateTime<Utc>,
    pub(crate) claimed_at: Option<DateTime<Utc>>,
    pub(crate) sent_request_id: Option<i64>,
    pub(crate) sent_at: Option<DateTime<Utc>>,
    pub(crate) consumed_at: Option<DateTime<Utc>>,
    pub(crate) dead_lettered_at: Option<DateTime<Utc>>,
    pub(crate) channel_delivered_at: Option<DateTime<Utc>>,
    pub(crate) delivery_attempts: i32,
    pub(crate) thread_uuid: uuid::Uuid,
    pub(crate) in_reply_to_uuid: Option<uuid::Uuid>,
    pub(crate) context_items: sqlx::types::Json<Vec<ContextItem>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolveThreadOutcome {
    Resolved(DateTime<Utc>),
    NotFound,
}
