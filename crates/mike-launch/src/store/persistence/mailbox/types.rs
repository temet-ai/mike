use chrono::{DateTime, Utc};

pub type ContextItems = sqlx::types::Json<Vec<ContextItem>>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextItem {
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub label: Option<String>,
}

impl ContextItem {
    pub const KINDS: &'static [&'static str] = &[
        "request", "session", "thread", "message", "commit", "file", "issue", "url", "note",
    ];
}

#[derive(Debug, sqlx::FromRow)]
pub struct ReceiptEvidenceRow {
    pub request_id: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MailboxMessageRow {
    pub message_uuid: uuid::Uuid,
    pub to_session_uuid: uuid::Uuid,
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
    pub thread_uuid: Option<uuid::Uuid>,
    pub in_reply_to_uuid: Option<uuid::Uuid>,
}

#[derive(Debug, Clone)]
pub struct ReloadedMailboxRow {
    pub row: MailboxMessageRow,
    pub first_for_recipient: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct InboxCounts {
    pub unconsumed: i64,
    pub unclaimed: i64,
    pub dead_lettered_outbox: i64,
}

impl MailboxMessageRow {
    pub fn to_reminder(&self) -> crate::store::models::SystemReminder {
        let from = self.from_session_uuid.map(|u| u.to_string());
        crate::store::models::SystemReminder {
            payload: MailboxEnvelope {
                from_session_uuid: from.as_deref(),
                queued_at: &self.queued_at,
                body: &self.body,
                to_session_uuid: self.to_session_uuid,
                message_uuid: self.message_uuid,
                thread: None,
            }
            .render(),
            source: crate::store::models::ReminderSource::SessionMailbox {
                message_uuid: self.message_uuid,
                from_session_uuid: from,
            },
        }
    }
}

pub struct MailboxEnvelope<'a> {
    pub from_session_uuid: Option<&'a str>,
    pub queued_at: &'a DateTime<Utc>,
    pub body: &'a str,
    pub to_session_uuid: uuid::Uuid,
    pub message_uuid: uuid::Uuid,
    pub thread: Option<(&'a str, uuid::Uuid)>,
}

impl MailboxEnvelope<'_> {
    pub fn render(&self) -> String {
        self.render_as(None)
    }

    pub fn render_as(&self, sender: Option<&str>) -> String {
        format!(
            "<system-reminder>\n{}\n</system-reminder>",
            self.letter_as(sender)
        )
    }

    pub fn letter_as(&self, sender: Option<&str>) -> String {
        let who = match (sender, self.from_session_uuid) {
            (Some(label), _) => label.to_owned(),
            (None, Some(from)) => format!("session {from}"),
            (None, None) => UNKNOWN_SENDER.to_owned(),
        };
        format!("{MAIL_LEAD}{who}: {}", self.body)
    }
}

pub use crate::wire::mail_text::{MAIL_LEAD, MIKE_SENDER, UNKNOWN_SENDER};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedeliveryOutcome {
    pub delivery_attempts: i32,
    pub dead_lettered: bool,
    pub attempted: bool,
}

#[derive(Debug)]
pub struct StoredMessageRow {
    pub message_uuid: uuid::Uuid,
    pub to_session_uuid: uuid::Uuid,
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
    pub to_session_id: i64,
    pub context_items: sqlx::types::Json<Vec<ContextItem>>,
    pub thread_uuid: Option<uuid::Uuid>,
    pub in_reply_to_uuid: Option<uuid::Uuid>,
}

impl StoredMessageRow {
    pub fn with_activity(
        self,
        recipient_last_request_at: Option<DateTime<Utc>>,
        recipient_last_eligible_turn_at: Option<DateTime<Utc>>,
    ) -> MailboxMessageWithActivity {
        MailboxMessageWithActivity {
            row: MailboxMessageRow {
                message_uuid: self.message_uuid,
                to_session_uuid: self.to_session_uuid,
                from_session_uuid: self.from_session_uuid,
                body: self.body,
                queued_at: self.queued_at,
                claimed_at: self.claimed_at,
                sent_request_id: self.sent_request_id,
                sent_at: self.sent_at,
                consumed_at: self.consumed_at,
                dead_lettered_at: self.dead_lettered_at,
                channel_delivered_at: self.channel_delivered_at,
                delivery_attempts: self.delivery_attempts,
                thread_uuid: self.thread_uuid,
                in_reply_to_uuid: self.in_reply_to_uuid,
            },
            recipient_last_request_at,
            recipient_last_eligible_turn_at,
            context_items: self.context_items.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MailboxMessageWithActivity {
    pub row: MailboxMessageRow,
    pub recipient_last_request_at: Option<DateTime<Utc>>,
    pub recipient_last_eligible_turn_at: Option<DateTime<Utc>>,
    pub context_items: Vec<ContextItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RecipientActivity {
    pub last_request_at: Option<DateTime<Utc>>,
    pub last_eligible_turn_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct MailboxForSessionWithActivity {
    pub inbox: Vec<MailboxMessageWithActivity>,
    pub outbox: Vec<MailboxMessageWithActivity>,
}

#[derive(Debug, Clone)]
pub struct InsertedMessage {
    pub message_uuid: uuid::Uuid,
    pub from_session_uuid: Option<uuid::Uuid>,
    pub queued_at: DateTime<Utc>,
}

pub struct ThreadedMessageInsert<'a> {
    pub message_uuid: uuid::Uuid,
    pub to_session_id: Option<i64>,
    pub from_session_id: Option<i64>,
    pub body: &'a str,
    pub thread_id: Option<i64>,
    pub in_reply_to_id: Option<i64>,
    pub held: bool,
}

#[derive(Debug, Clone)]
pub enum SessionIdResolution {
    Resolved(uuid::Uuid),
    Ambiguous(Vec<uuid::Uuid>),
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    Consumed(DateTime<Utc>),
    NeverDelivered,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReapOutcome {
    pub dead_lettered: i64,
}

#[derive(Debug, Clone)]
pub struct ClaimedChannelMessageRow {
    pub message_uuid: uuid::Uuid,
    pub to_session_uuid: uuid::Uuid,
    pub from_session_uuid: Option<uuid::Uuid>,
    pub from_display_name: Option<String>,
    pub from_role: Option<String>,
    pub body: String,
    pub queued_at: DateTime<Utc>,
    pub delivery_attempts: i32,
    pub thread_uuid: Option<uuid::Uuid>,
    pub in_reply_to_uuid: Option<uuid::Uuid>,
    pub first_for_recipient: bool,
    pub from_label: Option<String>,
}

impl ClaimedChannelMessageRow {
    pub fn envelope(&self) -> String {
        let from = self.from_session_uuid.map(|u| u.to_string());
        MailboxEnvelope {
            from_session_uuid: from.as_deref(),
            queued_at: &self.queued_at,
            body: &self.body,
            to_session_uuid: self.to_session_uuid,
            message_uuid: self.message_uuid,
            thread: None,
        }
        .letter_as(self.from_label.as_deref())
    }
}
