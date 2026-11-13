use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::http::StatusCode;
use axum::response::Response;

use crate::api::handlers::mailbox_error;
use crate::store::persistence::PersistHandle;

pub enum RecipientPlan {
    Deliver(Vec<uuid::Uuid>),
    PostOnly,
}

pub async fn resolve_recipients(
    handle: &PersistHandle,
    thread_id: i64,
    thread_uuid: uuid::Uuid,
    to: &Option<Vec<String>>,
    from_session_uuid: Option<uuid::Uuid>,
) -> Result<RecipientPlan, Response> {
    let Some(raw_list) = to else {
        return default_to_participants(handle, thread_id, thread_uuid, from_session_uuid).await;
    };
    if raw_list.is_empty() {
        return Ok(RecipientPlan::PostOnly);
    }
    let mut out = Vec::with_capacity(raw_list.len());
    for raw in raw_list {
        let u = uuid::Uuid::parse_str(raw).map_err(|_| {
            mailbox_error(
                StatusCode::BAD_REQUEST,
                "invalid_to",
                format!("to contains an invalid UUID: {raw}"),
            )
        })?;
        out.push(u);
    }
    Ok(RecipientPlan::Deliver(out))
}

async fn default_to_participants(
    handle: &PersistHandle,
    thread_id: i64,
    thread_uuid: uuid::Uuid,
    from_session_uuid: Option<uuid::Uuid>,
) -> Result<RecipientPlan, Response> {
    let participants = match handle.launch().fetch_thread_participants(thread_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "post_thread_message: participants fetch failed");
            return Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve thread participants".to_string(),
            ));
        }
    };
    let recipients: Vec<uuid::Uuid> = participants
        .into_iter()
        .filter(|u| Some(*u) != from_session_uuid)
        .collect();
    if recipients.is_empty() {
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "no_recipients",
            "the thread has no other participants to deliver to — pass `to` explicitly".to_string(),
        ));
    }
    Ok(RecipientPlan::Deliver(recipients))
}
