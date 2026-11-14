use crate::api::handlers::mailbox_dto::invalid_tags;
use crate::api::handlers::mailbox_dto::mailbox_error;
use crate::api::handlers::mailbox_thread_dto::FailedDeliveryDto;
use crate::api::handlers::mailbox_thread_dto::ThreadDeliveryDto;
use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PostThreadBody {
    pub subject: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, alias = "from", alias = "from_session_uuid")]
    pub from_session_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notify: Vec<String>,
}

#[derive(Serialize)]
pub struct PostThreadResponse {
    pub thread_uuid: uuid::Uuid,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliveries: Option<Vec<ThreadDeliveryDto>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<FailedDeliveryDto>,
}

pub async fn post_thread(
    State(state): State<BaseState>,
    Json(body): Json<PostThreadBody>,
) -> Result<(StatusCode, Json<PostThreadResponse>), Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; threads are not available".to_string(),
        ));
    };

    let (subject, trimmed_body) = validate_thread_body(&body).map_err(|e| *e)?;
    let (from_session_uuid, created_by_session_id) =
        resolve_thread_sender(handle, body.from_session_id.as_deref()).await?;
    let notify_uuids = parse_notify_uuids(&body.notify).map_err(|e| *e)?;
    let project_id = resolve_project_id_by_path(handle, body.project_path.as_deref()).await?;

    let thread = create_thread_row(
        handle,
        subject,
        project_id,
        created_by_session_id,
        &body.tags,
    )
    .await?;

    let (deliveries, failed) = if let Some(msg_body) = trimmed_body {
        deliver_thread_first_message(
            handle,
            &state,
            ThreadNotifyArgs {
                notify_uuids: &notify_uuids,
                thread_id: thread.id,
                thread_uuid: thread.thread_uuid,
                subject: &thread.subject,
                created_by_session_id,
                from_session_uuid,
                msg_body,
            },
        )
        .await
    } else {
        (Vec::new(), Vec::new())
    };

    Ok((
        StatusCode::CREATED,
        Json(PostThreadResponse {
            thread_uuid: thread.thread_uuid,
            subject: thread.subject,
            deliveries: (!notify_uuids.is_empty()).then_some(deliveries),
            failed,
        }),
    ))
}

fn validate_thread_body(body: &PostThreadBody) -> Result<(&str, Option<&str>), Box<Response>> {
    let subject = body.subject.trim();
    if subject.is_empty() {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "empty_subject",
            "subject must not be empty".to_string(),
        )));
    }

    let trimmed_body = body
        .body
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    if trimmed_body.is_none() && !body.notify.is_empty() {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "empty_message",
            "body must not be empty when notify is non-empty".to_string(),
        )));
    }
    if let Some((code, message)) = invalid_tags(&body.tags) {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            code,
            message,
        )));
    }
    Ok((subject, trimmed_body))
}

struct NewThread {
    id: i64,
    thread_uuid: uuid::Uuid,
    subject: String,
}

async fn create_thread_row(
    handle: &crate::store::persistence::PersistHandle,
    subject: &str,
    project_id: Option<i64>,
    created_by_session_id: Option<i64>,
    tags: &[String],
) -> Result<NewThread, Response> {
    let thread = insert_thread_row(handle, subject, project_id, created_by_session_id).await?;
    attach_thread_tags_best_effort(handle, thread.id, thread.thread_uuid, tags).await;
    Ok(thread)
}

async fn insert_thread_row(
    handle: &crate::store::persistence::PersistHandle,
    subject: &str,
    project_id: Option<i64>,
    created_by_session_id: Option<i64>,
) -> Result<NewThread, Response> {
    match handle
        .launch()
        .create_thread(
            uuid::Uuid::new_v4(),
            subject,
            project_id,
            created_by_session_id,
        )
        .await
    {
        Ok(c) => Ok(NewThread {
            id: c.id,
            thread_uuid: c.thread_uuid,
            subject: c.subject,
        }),
        Err(e) => {
            tracing::warn!(error = %e, "post_thread: create_thread failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to create thread".to_string(),
            ))
        }
    }
}

async fn attach_thread_tags_best_effort(
    handle: &crate::store::persistence::PersistHandle,
    thread_id: i64,
    thread_uuid: uuid::Uuid,
    tags: &[String],
) {
    if let Err(e) = handle.launch().attach_tags_to_thread(thread_id, tags).await {
        tracing::warn!(error = %e, %thread_uuid, "post_thread: tag attach failed");
    }
}

async fn resolve_thread_sender(
    handle: &crate::store::persistence::PersistHandle,
    from_session_id: Option<&str>,
) -> Result<(Option<uuid::Uuid>, Option<i64>), Response> {
    let Some(raw) = from_session_id else {
        return Ok((None, None));
    };
    let from_session_uuid = uuid::Uuid::parse_str(raw).map_err(|_| {
        mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_from_session_id",
            format!("from_session_id is not a valid UUID: {raw}"),
        )
    })?;
    match handle
        .launch()
        .resolve_session_core(from_session_uuid)
        .await
    {
        Ok(Some((id, _))) => Ok((Some(from_session_uuid), Some(id))),
        Ok(None) => Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "from_session_not_found",
            format!("no session with uuid {from_session_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(error = %e, %from_session_uuid, "post_thread: sender lookup failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve sender session".to_string(),
            ))
        }
    }
}

fn parse_notify_uuids(notify: &[String]) -> Result<Vec<uuid::Uuid>, Box<Response>> {
    let mut out = Vec::with_capacity(notify.len());
    for raw in notify {
        let u = uuid::Uuid::parse_str(raw).map_err(|_| {
            Box::new(mailbox_error(
                StatusCode::BAD_REQUEST,
                "invalid_notify",
                format!("notify contains an invalid UUID: {raw}"),
            ))
        })?;
        out.push(u);
    }
    Ok(out)
}

async fn resolve_project_id_by_path(
    handle: &crate::store::persistence::PersistHandle,
    project_path: Option<&str>,
) -> Result<Option<i64>, Response> {
    let Some(path) = project_path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    handle.find_project_id_by_path(path).await.map_err(|e| {
        tracing::warn!(error = %e, project = path, "post_thread: project_path resolution failed");
        mailbox_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database_error",
            "failed to resolve project".to_string(),
        )
    })
}

async fn insert_post_only_message(
    handle: &crate::store::persistence::PersistHandle,
    thread_id: i64,
    created_by_session_id: Option<i64>,
    msg_body: &str,
    thread_uuid: uuid::Uuid,
) {
    if let Err(e) = handle
        .launch()
        .insert_threaded_message_with_context(
            crate::store::persistence::mailbox::ThreadedMessageInsert {
                message_uuid: uuid::Uuid::new_v4(),
                to_session_id: None,
                from_session_id: created_by_session_id,
                body: msg_body,
                thread_id: Some(thread_id),
                in_reply_to_id: None,
                held: false,
            },
            &[],
        )
        .await
    {
        tracing::warn!(error = %e, %thread_uuid, "post_thread: post-only insert failed");
    }
}

struct ThreadNotifyArgs<'a> {
    notify_uuids: &'a [uuid::Uuid],
    thread_id: i64,
    thread_uuid: uuid::Uuid,
    subject: &'a str,
    created_by_session_id: Option<i64>,
    from_session_uuid: Option<uuid::Uuid>,
    msg_body: &'a str,
}

async fn deliver_thread_notifications(
    handle: &crate::store::persistence::PersistHandle,
    state: &BaseState,
    args: &ThreadNotifyArgs<'_>,
) -> (Vec<ThreadDeliveryDto>, Vec<FailedDeliveryDto>) {
    let mut deliveries = Vec::new();
    let mut failed = Vec::new();
    for to_uuid in args.notify_uuids {
        match deliver_one_thread_notification(handle, state, args, *to_uuid).await {
            Ok(delivery) => deliveries.push(delivery),
            Err(failure) => failed.push(failure),
        }
    }
    (deliveries, failed)
}

async fn deliver_one_thread_notification(
    handle: &crate::store::persistence::PersistHandle,
    state: &BaseState,
    args: &ThreadNotifyArgs<'_>,
    to_uuid: uuid::Uuid,
) -> Result<ThreadDeliveryDto, FailedDeliveryDto> {
    let to_session_id = resolve_notify_recipient_id(handle, to_uuid).await?;
    let (message_uuid, queued_at) =
        insert_notify_message(handle, args, to_session_id, to_uuid).await?;
    let from = args.from_session_uuid.map(|u| u.to_string());
    let reminder = crate::store::models::SystemReminder {
        payload: crate::store::persistence::mailbox::MailboxEnvelope {
            from_session_uuid: from.as_deref(),
            queued_at: &queued_at,
            body: args.msg_body,
            to_session_uuid: to_uuid,
            message_uuid,
            thread: Some((args.subject, args.thread_uuid)),
        }
        .render(),
        source: crate::store::models::ReminderSource::SessionMailbox {
            message_uuid,
            from_session_uuid: from,
        },
    };
    state
        .store
        .write()
        .await
        .enqueue_reminder(&to_uuid.to_string(), reminder);
    Ok(ThreadDeliveryDto {
        message_uuid,
        to_session_uuid: to_uuid,
    })
}

async fn resolve_notify_recipient_id(
    handle: &crate::store::persistence::PersistHandle,
    to_uuid: uuid::Uuid,
) -> Result<i64, FailedDeliveryDto> {
    match handle.launch().resolve_session_core(to_uuid).await {
        Ok(Some((id, _))) => Ok(id),
        Ok(None) => Err(failed_notification(to_uuid, "session_not_found")),
        Err(e) => {
            tracing::warn!(error = %e, %to_uuid, "post_thread: recipient lookup failed");
            Err(failed_notification(to_uuid, "database_error"))
        }
    }
}

async fn insert_notify_message(
    handle: &crate::store::persistence::PersistHandle,
    args: &ThreadNotifyArgs<'_>,
    to_session_id: i64,
    to_uuid: uuid::Uuid,
) -> Result<(uuid::Uuid, chrono::DateTime<chrono::Utc>), FailedDeliveryDto> {
    let message_uuid = uuid::Uuid::new_v4();
    match handle
        .launch()
        .insert_threaded_message_with_context(
            crate::store::persistence::mailbox::ThreadedMessageInsert {
                message_uuid,
                to_session_id: Some(to_session_id),
                from_session_id: args.created_by_session_id,
                body: args.msg_body,
                thread_id: Some(args.thread_id),
                in_reply_to_id: None,
                held: false,
            },
            &[],
        )
        .await
    {
        Ok(Some(queued_at)) => Ok((message_uuid, queued_at)),
        Ok(None) => Err(failed_notification(to_uuid, "recipient_closed")),
        Err(e) => {
            tracing::warn!(error = %e, %to_uuid, "post_thread: notify insert failed");
            Err(failed_notification(to_uuid, "insert_failed"))
        }
    }
}

async fn deliver_thread_first_message(
    handle: &crate::store::persistence::PersistHandle,
    state: &BaseState,
    args: ThreadNotifyArgs<'_>,
) -> (Vec<ThreadDeliveryDto>, Vec<FailedDeliveryDto>) {
    let result = deliver_thread_notifications(handle, state, &args).await;
    if result.0.is_empty() {
        insert_post_only_message(
            handle,
            args.thread_id,
            args.created_by_session_id,
            args.msg_body,
            args.thread_uuid,
        )
        .await;
    }
    result
}

fn failed_notification(to_session_uuid: uuid::Uuid, reason: &'static str) -> FailedDeliveryDto {
    FailedDeliveryDto {
        to_session_uuid,
        reason,
    }
}
