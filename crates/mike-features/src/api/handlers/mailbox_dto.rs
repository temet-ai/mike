use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
pub struct MailboxErrorBody {
    pub error: &'static str,
    pub message: String,
}

pub fn mailbox_error(status: StatusCode, error: &'static str, message: String) -> Response {
    (status, Json(MailboxErrorBody { error, message })).into_response()
}

pub const MAX_CONTEXT_ITEMS: usize = 32;
pub const MAX_TAGS: usize = 16;
pub const MAX_CONTEXT_FIELD_CHARS: usize = 2048;
pub const MAX_TAG_CODE_CHARS: usize = 64;

pub fn is_http_or_https_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

pub fn invalid_context_items(
    items: &[crate::store::persistence::mailbox::ContextItem],
) -> Option<(&'static str, String)> {
    if items.len() > MAX_CONTEXT_ITEMS {
        return Some((
            "too_many_context_items",
            format!(
                "context carries {} items, max {MAX_CONTEXT_ITEMS} per send",
                items.len()
            ),
        ));
    }
    for item in items {
        if !crate::store::persistence::mailbox::ContextItem::KINDS.contains(&item.kind.as_str()) {
            return Some((
                "invalid_context_kind",
                format!(
                    "unknown context kind `{}` — must be one of {:?}",
                    item.kind,
                    crate::store::persistence::mailbox::ContextItem::KINDS
                ),
            ));
        }
        if item.kind == "url" && !is_http_or_https_url(&item.value) {
            return Some((
                "invalid_context_url",
                format!(
                    "context item of kind `url` must be an http(s) URL, got `{}`",
                    item.value
                ),
            ));
        }
        if item.value.chars().count() > MAX_CONTEXT_FIELD_CHARS {
            return Some((
                "context_value_too_long",
                format!(
                    "context item value is {} chars, max {MAX_CONTEXT_FIELD_CHARS}",
                    item.value.chars().count()
                ),
            ));
        }
        if let Some(label) = &item.label
            && label.chars().count() > MAX_CONTEXT_FIELD_CHARS
        {
            return Some((
                "context_label_too_long",
                format!(
                    "context item label is {} chars, max {MAX_CONTEXT_FIELD_CHARS}",
                    label.chars().count()
                ),
            ));
        }
    }
    None
}

pub fn invalid_tags(tags: &[String]) -> Option<(&'static str, String)> {
    if tags.len() > MAX_TAGS {
        return Some((
            "too_many_tags",
            format!("tags carries {} codes, max {MAX_TAGS} per send", tags.len()),
        ));
    }
    for tag in tags {
        if tag.chars().count() > MAX_TAG_CODE_CHARS {
            return Some((
                "tag_code_too_long",
                format!(
                    "tag code `{tag}` is {} chars, max {MAX_TAG_CODE_CHARS}",
                    tag.chars().count()
                ),
            ));
        }
    }
    None
}

#[derive(Serialize)]
pub struct PostSessionMessageResponse {
    pub message_uuid: uuid::Uuid,
    pub to_session_uuid: uuid::Uuid,
    pub from_session_uuid: Option<uuid::Uuid>,
    pub queued_at: String,
    pub recipient_liveness: Option<&'static str>,
    pub recipient_last_request_at: Option<String>,
    pub recipient_last_eligible_turn_at: Option<String>,
    pub delivery_state: Option<&'static str>,
}

#[derive(Serialize)]
pub struct MailboxMessageDto {
    pub message_uuid: uuid::Uuid,
    pub from_session_uuid: Option<uuid::Uuid>,
    pub to_session_uuid: Option<uuid::Uuid>,
    pub body: String,
    pub queued_at: String,
    pub claimed_at: Option<String>,
    pub sent_request_id: Option<i64>,
    pub sent_at: Option<String>,
    pub consumed_at: Option<String>,
    pub dead_lettered_at: Option<String>,
    pub channel_delivered_at: Option<String>,
    pub delivery_attempts: i32,
    pub status: &'static str,
    pub pending_minutes: Option<i64>,
    pub stale: Option<bool>,
    pub recipient_last_request_at: Option<String>,
    pub recipient_last_eligible_turn_at: Option<String>,
    pub delivery_state: Option<&'static str>,
    pub thread_uuid: Option<uuid::Uuid>,
    pub in_reply_to_uuid: Option<uuid::Uuid>,
    pub context_items: Vec<crate::store::persistence::mailbox::ContextItem>,
}

pub struct MailboxLifecycle {
    pub consumed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub dead_lettered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub sent_request_id: Option<i64>,
    pub channel_delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub queued_at: chrono::DateTime<chrono::Utc>,
}

pub fn mailbox_status_and_staleness(
    lifecycle: MailboxLifecycle,
    now: chrono::DateTime<chrono::Utc>,
    stale_threshold_minutes: i64,
) -> (&'static str, Option<i64>, Option<bool>) {
    let status = if lifecycle.consumed_at.is_some() {
        "consumed"
    } else if lifecycle.dead_lettered_at.is_some() {
        "dead_lettered"
    } else if lifecycle.sent_request_id.is_some() || lifecycle.channel_delivered_at.is_some() {
        "delivered"
    } else if lifecycle.claimed_at.is_some() {
        "claimed"
    } else {
        "pending"
    };
    let pending_minutes = (status == "pending").then(|| (now - lifecycle.queued_at).num_minutes());
    let stale = pending_minutes.map(|m| m >= stale_threshold_minutes);
    (status, pending_minutes, stale)
}

pub fn mailbox_message_dto(
    r: crate::store::persistence::mailbox::MailboxMessageRow,
    now: chrono::DateTime<chrono::Utc>,
    stale_threshold_minutes: i64,
) -> MailboxMessageDto {
    let (status, pending_minutes, stale) = mailbox_status_and_staleness(
        MailboxLifecycle {
            consumed_at: r.consumed_at,
            dead_lettered_at: r.dead_lettered_at,
            sent_request_id: r.sent_request_id,
            channel_delivered_at: r.channel_delivered_at,
            claimed_at: r.claimed_at,
            queued_at: r.queued_at,
        },
        now,
        stale_threshold_minutes,
    );
    MailboxMessageDto {
        message_uuid: r.message_uuid,
        from_session_uuid: r.from_session_uuid,
        to_session_uuid: Some(r.to_session_uuid),
        body: r.body,
        queued_at: r.queued_at.to_rfc3339(),
        claimed_at: r.claimed_at.map(|t| t.to_rfc3339()),
        sent_request_id: r.sent_request_id,
        sent_at: r.sent_at.map(|t| t.to_rfc3339()),
        consumed_at: r.consumed_at.map(|t| t.to_rfc3339()),
        dead_lettered_at: r.dead_lettered_at.map(|t| t.to_rfc3339()),
        channel_delivered_at: r.channel_delivered_at.map(|t| t.to_rfc3339()),
        delivery_attempts: r.delivery_attempts,
        status,
        pending_minutes,
        stale,
        recipient_last_request_at: None,
        recipient_last_eligible_turn_at: None,
        delivery_state: None,
        thread_uuid: r.thread_uuid,
        in_reply_to_uuid: r.in_reply_to_uuid,
        context_items: Vec::new(),
    }
}

pub fn mailbox_message_dto_with_activity(
    r: crate::store::persistence::mailbox::MailboxMessageWithActivity,
    now: chrono::DateTime<chrono::Utc>,
    stale_threshold_minutes: i64,
    delivery_ttl_minutes: i64,
) -> MailboxMessageDto {
    let recipient_last_request_at = r.recipient_last_request_at;
    let recipient_last_eligible_turn_at = r.recipient_last_eligible_turn_at;
    let context_items = r.context_items;
    let mut dto = mailbox_message_dto(r.row, now, stale_threshold_minutes);
    dto.context_items = context_items;
    let undelivered_pending = dto.sent_at.is_none()
        && dto.channel_delivered_at.is_none()
        && dto.consumed_at.is_none()
        && dto.dead_lettered_at.is_none();
    dto.delivery_state = undelivered_pending.then(|| {
        let within_ttl = recipient_last_request_at
            .is_some_and(|t| now - t <= chrono::Duration::minutes(delivery_ttl_minutes));
        if within_ttl {
            "awaiting_eligible_turn"
        } else {
            "recipient_inactive"
        }
    });
    dto.recipient_last_request_at = recipient_last_request_at.map(|t| t.to_rfc3339());
    dto.recipient_last_eligible_turn_at = recipient_last_eligible_turn_at.map(|t| t.to_rfc3339());
    dto
}

#[derive(Serialize)]
pub struct SessionMessagesDto {
    pub session_id: uuid::Uuid,
    pub inbox: Vec<MailboxMessageDto>,
    pub outbox: Vec<MailboxMessageDto>,
}

pub fn thread_message_dto(
    r: crate::store::persistence::mailbox::ThreadMessageRow,
    now: chrono::DateTime<chrono::Utc>,
    stale_threshold_minutes: i64,
) -> MailboxMessageDto {
    let (status, pending_minutes, stale) = mailbox_status_and_staleness(
        MailboxLifecycle {
            consumed_at: r.consumed_at,
            dead_lettered_at: r.dead_lettered_at,
            sent_request_id: r.sent_request_id,
            channel_delivered_at: r.channel_delivered_at,
            claimed_at: r.claimed_at,
            queued_at: r.queued_at,
        },
        now,
        stale_threshold_minutes,
    );
    MailboxMessageDto {
        message_uuid: r.message_uuid,
        from_session_uuid: r.from_session_uuid,
        to_session_uuid: r.to_session_uuid,
        body: r.body,
        queued_at: r.queued_at.to_rfc3339(),
        claimed_at: r.claimed_at.map(|t| t.to_rfc3339()),
        sent_request_id: r.sent_request_id,
        sent_at: r.sent_at.map(|t| t.to_rfc3339()),
        consumed_at: r.consumed_at.map(|t| t.to_rfc3339()),
        dead_lettered_at: r.dead_lettered_at.map(|t| t.to_rfc3339()),
        channel_delivered_at: r.channel_delivered_at.map(|t| t.to_rfc3339()),
        delivery_attempts: r.delivery_attempts,
        status,
        pending_minutes,
        stale,
        recipient_last_request_at: None,
        recipient_last_eligible_turn_at: None,
        delivery_state: None,
        thread_uuid: Some(r.thread_uuid),
        in_reply_to_uuid: r.in_reply_to_uuid,
        context_items: r.context_items.0,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParticipantIdentityDto {
    pub session_uuid: uuid::Uuid,
    pub label: String,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub agent_type: Option<String>,
}

pub fn participant_label(
    display_name: Option<&str>,
    title: Option<&str>,
    role: Option<&str>,
    agent_type: Option<&str>,
    session_uuid: uuid::Uuid,
) -> String {
    [display_name, title, role, agent_type]
        .into_iter()
        .find_map(|s| s.map(str::trim).filter(|s| !s.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| session_uuid.to_string()[..8].to_string())
}

pub fn build_identity_index(
    rows: Vec<crate::store::persistence::mailbox::ParticipantIdentityRow>,
) -> std::collections::HashMap<uuid::Uuid, ParticipantIdentityDto> {
    rows.into_iter()
        .map(|r| {
            let label = participant_label(
                r.display_name.as_deref(),
                r.title.as_deref(),
                r.role.as_deref(),
                r.agent_type.as_deref(),
                r.session_uuid,
            );
            (
                r.session_uuid,
                ParticipantIdentityDto {
                    session_uuid: r.session_uuid,
                    label,
                    display_name: r.display_name,
                    role: r.role,
                    agent_type: r.agent_type,
                },
            )
        })
        .collect()
}

#[derive(Serialize)]
pub struct ThreadSummaryDto {
    pub thread_uuid: uuid::Uuid,
    pub subject: String,
    pub created_at: String,
    pub last_message_at: String,
    pub resolved_at: Option<String>,
    pub created_by_session_uuid: Option<uuid::Uuid>,
    pub participants: Vec<uuid::Uuid>,
    pub participant_identities: Vec<ParticipantIdentityDto>,
    pub tags: Vec<String>,
    pub message_count: i64,
    pub unread_count: i64,
    pub worst_delivery_state: &'static str,
    pub has_stale_pending: bool,
    pub project: Option<ThreadProjectDto>,
    pub fleet: Option<ThreadFleetDto>,
}

#[derive(Serialize)]
pub struct ThreadProjectDto {
    pub project_uuid: uuid::Uuid,
    pub name: String,
    pub path: String,
    pub pinned: bool,
}

#[derive(Serialize)]
pub struct ThreadFleetDto {
    pub fleet_uuid: uuid::Uuid,
    pub slug: String,
    pub state: String,
}

pub fn worst_delivery_state_from_rank(rank: Option<i32>) -> &'static str {
    match rank {
        Some(0) => "dead_lettered",
        Some(2) => "claimed",
        Some(3) => "delivered",
        Some(4) => "consumed",
        _ => "pending",
    }
}

#[cfg(test)]
pub mod status_tests {
    use super::{MailboxLifecycle, mailbox_status_and_staleness};

    fn lifecycle(
        now: chrono::DateTime<chrono::Utc>,
        channel_delivered: bool,
        consumed: bool,
    ) -> MailboxLifecycle {
        MailboxLifecycle {
            consumed_at: consumed.then_some(now),
            dead_lettered_at: None,
            sent_request_id: None,
            channel_delivered_at: channel_delivered.then_some(now),
            claimed_at: None,
            queued_at: now - chrono::Duration::minutes(30),
        }
    }

    #[test]
    fn channel_pushed_message_is_delivered_not_pending() {
        let now = chrono::Utc::now();
        let (status, pending_minutes, stale) =
            mailbox_status_and_staleness(lifecycle(now, true, false), now, 10);
        assert_eq!(status, "delivered");
        assert_eq!(pending_minutes, None);
        assert_eq!(stale, None);
    }

    #[test]
    fn consumed_channel_message_is_consumed() {
        let now = chrono::Utc::now();
        let (status, _, _) = mailbox_status_and_staleness(lifecycle(now, true, true), now, 10);
        assert_eq!(status, "consumed");
    }

    #[test]
    fn unpushed_message_stays_pending_and_goes_stale() {
        let now = chrono::Utc::now();
        let (status, _, stale) =
            mailbox_status_and_staleness(lifecycle(now, false, false), now, 10);
        assert_eq!(status, "pending");
        assert_eq!(stale, Some(true));
    }
}
