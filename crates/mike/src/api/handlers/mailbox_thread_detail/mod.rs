use super::*;

mod reply_context;
mod reply_delivery;
mod reply_recipients;

use reply_context::build_reply_context;
use reply_recipients::{RecipientPlan, resolve_recipients};

pub(crate) async fn get_thread(
    State(state): State<AppState>,
    Path(thread_uuid): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Response {
    let Some(handle) = state.persist() else {
        return mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; threads are not available".to_string(),
        );
    };
    let poll = crate::api::poll_answers::Poll {
        key: format!("thread:{thread_uuid}"),
        freshness: crate::api::poll_answers::MAILBOX_THREADS,
        scope: Vec::new(),
        headers: &headers,
    };
    crate::api::poll_answers::answer_json(&state, poll, || load_thread(handle, &state, thread_uuid))
        .await
}

async fn load_thread(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    thread_uuid: uuid::Uuid,
) -> Result<crate::api::poll_answers::Loaded<Bytes>, Response> {
    let now = chrono::Utc::now();
    let stale_threshold_minutes = state.config().mailbox_stale_minutes;

    let summary_row = fetch_thread_summary_row(handle, thread_uuid).await?;
    let messages = fetch_thread_transcript(
        handle,
        summary_row.id,
        now,
        stale_threshold_minutes,
        thread_uuid,
    )
    .await?;

    let identities = match handle
        .fetch_participant_identities(&summary_row.participant_uuids)
        .await
    {
        Ok(id_rows) => build_identity_index(id_rows),
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "get_thread: identity fetch failed");
            std::collections::HashMap::new()
        }
    };

    let sessions = summary_row
        .participant_uuids
        .iter()
        .map(ToString::to_string)
        .collect();
    let detail = ThreadDetailResponse {
        thread: thread_summary_dto(summary_row, now, stale_threshold_minutes, &identities),
        messages,
    };
    Ok(crate::api::poll_answers::Loaded {
        value: crate::api::poll_answers::json_bytes(&detail)?,
        sessions,
    })
}

async fn fetch_thread_summary_row(
    handle: &crate::store::persistence::PersistHandle,
    thread_uuid: uuid::Uuid,
) -> Result<crate::store::persistence::mailbox::ThreadSummaryRow, Response> {
    let filter = crate::store::persistence::mailbox::ThreadSummaryFilter {
        thread_uuid: Some(thread_uuid),
        ..Default::default()
    };
    match handle.fetch_thread_summaries(&filter).await {
        Ok(mut rows) => rows.pop().ok_or_else(|| {
            mailbox_error(
                StatusCode::NOT_FOUND,
                "thread_not_found",
                format!("no thread with uuid {thread_uuid}"),
            )
        }),
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "get_thread: summary fetch failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch thread".to_string(),
            ))
        }
    }
}

async fn fetch_thread_transcript(
    handle: &crate::store::persistence::PersistHandle,
    thread_id: i64,
    now: chrono::DateTime<chrono::Utc>,
    stale_threshold_minutes: i64,
    thread_uuid: uuid::Uuid,
) -> Result<Vec<MailboxMessageDto>, Response> {
    match handle.fetch_thread_messages(thread_id).await {
        Ok(rows) => Ok(rows
            .into_iter()
            .map(|r| thread_message_dto(r, now, stale_threshold_minutes))
            .collect()),
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "get_thread: messages fetch failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch thread messages".to_string(),
            ))
        }
    }
}

pub(crate) async fn post_thread_message(
    State(state): State<AppState>,
    Path(thread_uuid): Path<uuid::Uuid>,
    Json(body): Json<PostThreadMessageBody>,
) -> Result<(StatusCode, Json<ThreadDeliveriesResponse>), Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; threads are not available".to_string(),
        ));
    };

    if body.body.trim().is_empty() {
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "empty_message",
            "body must not be empty".to_string(),
        ));
    }
    if let Some((code, message)) = invalid_context_items(&body.context) {
        return Err(mailbox_error(StatusCode::BAD_REQUEST, code, message));
    }

    let ctx = build_reply_context(handle, thread_uuid, &body).await?;
    let plan = resolve_recipients(
        handle,
        ctx.thread_id,
        ctx.thread_uuid,
        &body.to,
        ctx.from_session_uuid,
    )
    .await?;

    match plan {
        RecipientPlan::PostOnly => reply_delivery::deliver_post_only(handle, &ctx, &body).await,
        RecipientPlan::Deliver(recipient_uuids) => {
            reply_delivery::deliver_to_recipients(handle, &state, &ctx, &recipient_uuids, &body)
                .await
        }
    }
}

pub(crate) async fn post_thread_resolve(
    State(state): State<AppState>,
    Path(thread_uuid): Path<uuid::Uuid>,
) -> Result<Json<ThreadResolvedResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; threads are not available".to_string(),
        ));
    };
    match handle.mark_thread_resolved(thread_uuid).await {
        Ok(crate::store::persistence::mailbox::ResolveThreadOutcome::Resolved(resolved_at)) => {
            Ok(Json(ThreadResolvedResponse {
                thread_uuid,
                resolved_at: resolved_at.to_rfc3339(),
            }))
        }
        Ok(crate::store::persistence::mailbox::ResolveThreadOutcome::NotFound) => {
            Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "thread_not_found",
                format!("no thread with uuid {thread_uuid}"),
            ))
        }
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "post_thread_resolve: DB update failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve thread".to_string(),
            ))
        }
    }
}
