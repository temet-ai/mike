use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;

pub(crate) fn thread_summary_dto(
    r: crate::store::persistence::mailbox::ThreadSummaryRow,
    now: chrono::DateTime<chrono::Utc>,
    stale_threshold_minutes: i64,
    identities: &std::collections::HashMap<uuid::Uuid, ParticipantIdentityDto>,
) -> ThreadSummaryDto {
    let has_stale_pending = r
        .oldest_pending_queued_at
        .is_some_and(|t| (now - t).num_minutes() >= stale_threshold_minutes);
    let participant_identities = r
        .participant_uuids
        .iter()
        .filter_map(|u| identities.get(u).cloned())
        .collect();
    let project = match (r.project_uuid, r.project_name, r.project_path) {
        (Some(project_uuid), Some(name), Some(path)) => Some(ThreadProjectDto {
            project_uuid,
            name,
            path,
            pinned: r.project_pinned.unwrap_or(false),
        }),
        _ => None,
    };
    let fleet = match (r.fleet_uuid, r.fleet_slug, r.fleet_state) {
        (Some(fleet_uuid), Some(slug), Some(state)) => Some(ThreadFleetDto {
            fleet_uuid,
            slug,
            state,
        }),
        _ => None,
    };
    ThreadSummaryDto {
        thread_uuid: r.thread_uuid,
        subject: r.subject,
        created_at: r.created_at.to_rfc3339(),
        last_message_at: r.last_message_at.to_rfc3339(),
        resolved_at: r.resolved_at.map(|t| t.to_rfc3339()),
        created_by_session_uuid: r.created_by_session_uuid,
        participants: r.participant_uuids,
        participant_identities,
        tags: r.tag_codes,
        message_count: r.message_count,
        unread_count: r.unread_count,
        worst_delivery_state: worst_delivery_state_from_rank(r.worst_rank),
        has_stale_pending,
        project,
        fleet,
    }
}

#[derive(Serialize)]
pub(crate) struct ThreadListResponse {
    pub(crate) threads: Vec<ThreadSummaryDto>,
    pub(crate) total_count: usize,
}

#[derive(Serialize)]
pub(crate) struct ThreadDetailResponse {
    pub(crate) thread: ThreadSummaryDto,
    pub(crate) messages: Vec<MailboxMessageDto>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListThreadsParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) tag: Option<String>,
    #[serde(default)]
    pub(crate) participant: Option<String>,
    #[serde(default)]
    pub(crate) resolved: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct PostThreadMessageBody {
    #[serde(default, alias = "from", alias = "from_session_uuid")]
    pub(crate) from_session_id: Option<String>,
    pub(crate) body: String,
    #[serde(default)]
    pub(crate) to: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) in_reply_to: Option<String>,
    #[serde(default)]
    pub(crate) context: Vec<crate::store::persistence::mailbox::ContextItem>,
}

#[derive(Serialize)]
pub(crate) struct ThreadDeliveryDto {
    pub(crate) message_uuid: uuid::Uuid,
    pub(crate) to_session_uuid: uuid::Uuid,
}

#[derive(Serialize)]
pub(crate) struct FailedDeliveryDto {
    pub(crate) to_session_uuid: uuid::Uuid,
    pub(crate) reason: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ThreadDeliveriesResponse {
    pub(crate) thread_uuid: uuid::Uuid,
    pub(crate) subject: String,
    pub(crate) deliveries: Vec<ThreadDeliveryDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) failed: Vec<FailedDeliveryDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) post_message_uuid: Option<uuid::Uuid>,
}

#[derive(Serialize)]
pub(crate) struct ThreadResolvedResponse {
    pub(crate) thread_uuid: uuid::Uuid,
    pub(crate) resolved_at: String,
}

#[derive(Serialize)]
pub(crate) struct TagDto {
    pub(crate) code: String,
    pub(crate) hue: Option<i16>,
    pub(crate) usage_count: i64,
}

#[derive(Serialize)]
pub(crate) struct TagListResponse {
    pub(crate) tags: Vec<TagDto>,
}

pub(crate) fn looks_like_session_id_prefix(raw: &str) -> bool {
    let len = raw.len();
    (8..=35).contains(&len) && raw.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

pub(crate) async fn resolve_mailbox_recipient(
    handle: &crate::store::persistence::PersistHandle,
    raw: &str,
) -> Result<uuid::Uuid, Response> {
    if let Ok(uuid) = uuid::Uuid::parse_str(raw) {
        return Ok(uuid);
    }
    if !looks_like_session_id_prefix(raw) {
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_session_id",
            format!(
                "Cannot parse `id` with value `{raw}`: UUID parsing failed: invalid length: \
                 found {}, expected length 32 for simple format, 36 for hyphenated format",
                raw.len()
            ),
        ));
    }
    match handle.launch().resolve_session_id_prefix(raw).await {
        Ok(crate::store::persistence::mailbox::SessionIdResolution::Resolved(uuid)) => Ok(uuid),
        Ok(crate::store::persistence::mailbox::SessionIdResolution::Ambiguous(candidates)) => {
            Err(ambiguous_session_id_response(raw, candidates))
        }
        Ok(crate::store::persistence::mailbox::SessionIdResolution::NotFound) => {
            Err(session_id_not_found_response(handle, raw).await)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                raw,
                "post_session_message: session id prefix resolution failed"
            );
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve session id".to_string(),
            ))
        }
    }
}

fn ambiguous_session_id_response(raw: &str, candidates: Vec<uuid::Uuid>) -> Response {
    let mut candidates: Vec<String> = candidates.iter().map(|u| u.to_string()).collect();
    candidates.sort();
    mailbox_error(
        StatusCode::CONFLICT,
        "ambiguous_session_id",
        format!(
            "id prefix `{raw}` matches {} sessions: {} — pass more of the UUID to disambiguate",
            candidates.len(),
            candidates.join(", ")
        ),
    )
}

async fn session_id_not_found_response(
    handle: &crate::store::persistence::PersistHandle,
    raw: &str,
) -> Response {
    if let Err(e) = handle
        .launch()
        .insert_failed_send(None, raw, "session_not_found", None)
        .await
    {
        tracing::warn!(
            error = %e,
            raw,
            "post_session_message: failed_sends audit insert failed"
        );
    }
    mailbox_error(
        StatusCode::NOT_FOUND,
        "session_not_found",
        format!(
            "no session matches id prefix `{raw}` — pass the exact UUID from \
             get_workspace_roster; do not pad short ids with zeros"
        ),
    )
}
