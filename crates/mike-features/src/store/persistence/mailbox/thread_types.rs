use chrono::{DateTime, Utc};

use super::types::ContextItem;

#[derive(Debug, Clone)]
pub struct ThreadCore {
    pub id: i64,
    pub thread_uuid: uuid::Uuid,
    pub subject: String,
}

#[derive(Debug, Clone)]
pub struct MessageCore {
    pub id: i64,
    pub thread_id: Option<i64>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub enum ThreadResolution {
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
pub struct ThreadSummaryRow {
    pub id: i64,
    pub thread_uuid: uuid::Uuid,
    pub subject: String,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_by_session_uuid: Option<uuid::Uuid>,
    pub participant_uuids: Vec<uuid::Uuid>,
    pub tag_codes: Vec<String>,
    pub message_count: i64,
    pub unread_count: i64,
    pub oldest_pending_queued_at: Option<DateTime<Utc>>,
    pub worst_rank: Option<i32>,
    pub project_uuid: Option<uuid::Uuid>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub project_pinned: Option<bool>,
    pub fleet_uuid: Option<uuid::Uuid>,
    pub fleet_slug: Option<String>,
    pub fleet_state: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ParticipantIdentityRow {
    pub session_uuid: uuid::Uuid,
    pub title: Option<String>,
    pub role: Option<String>,
    pub display_name: Option<String>,
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadSummaryFilter {
    pub thread_uuid: Option<uuid::Uuid>,
    pub participant_session_uuid: Option<uuid::Uuid>,
    pub project_ids: Option<Vec<i64>>,
    pub resolved: Option<bool>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThreadMessageRow {
    pub message_uuid: uuid::Uuid,
    pub to_session_uuid: Option<uuid::Uuid>,
    pub from_session_uuid: Option<uuid::Uuid>,
    pub body: String,
    pub queued_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub sent_request_id: Option<i64>,
    pub sent_at: Option<DateTime<Utc>>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub channel_delivered_at: Option<DateTime<Utc>>,
    pub delivery_attempts: i32,
    pub thread_uuid: uuid::Uuid,
    pub in_reply_to_uuid: Option<uuid::Uuid>,
    pub context_items: sqlx::types::Json<Vec<ContextItem>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveThreadOutcome {
    Resolved(DateTime<Utc>),
    NotFound,
}
