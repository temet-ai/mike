use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::HashMap;
use std::future::{Future, ready};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use tokio::sync::oneshot;
use tokio::time::Instant;
use uuid::Uuid;

use crate::api::client_serves::by_deadline;
use crate::reminders::launch_brief::brief_reminder;
use crate::state::AppState;
use crate::steerer::stage::client_served::mark_mailbox_served;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::QueuedLaunchBrief;

const MAIL_CLAIM_BUDGET: Duration = Duration::from_millis(600);

#[derive(Debug, Serialize)]
pub struct MailBlock {
    source: &'static str,
    message_id: Option<Uuid>,
    text: String,
}

#[derive(Debug, Serialize)]
pub struct MailClaimResponse {
    blocks: Vec<MailBlock>,
}

#[derive(Clone)]
struct ClaimJob {
    state: AppState,
    persist: PersistHandle,
    session: Uuid,
    source: &'static str,
}

pub async fn post_mail_claim(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MailClaimResponse>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    crate::api::client_serves::require_client_serves(&state, "client_serves_mail")?;
    let Some(persist) = state.persist().cloned() else {
        return Ok(Json(MailClaimResponse { blocks: Vec::new() }));
    };
    mark_mailbox_served(&id);
    let deadline = Instant::now() + MAIL_CLAIM_BUDGET;
    let briefs = by_deadline(deadline, pending_briefs(persist.clone(), session))
        .await
        .unwrap_or_default();
    let job = ClaimJob {
        state: state.clone(),
        persist: persist.clone(),
        session,
        source: "mailbox",
    };
    let mut replies = Vec::new();
    let mut seen = Vec::new();
    for (message_uuid, reminder) in split_queue(&state, &id).await {
        seen.push(message_uuid);
        let source = if briefs.contains_key(&message_uuid) {
            "launch_brief"
        } else {
            "mailbox"
        };
        replies.push(spawn_claim(
            ClaimJob {
                source,
                ..job.clone()
            },
            ready(reminder),
        ));
    }
    for (message_uuid, brief) in briefs {
        if seen.contains(&message_uuid) {
            continue;
        }
        let reminder = ready(brief_reminder(session, &brief));
        let source = "launch_brief";
        replies.push(spawn_claim(
            ClaimJob {
                source,
                ..job.clone()
            },
            reminder,
        ));
    }
    let mut blocks = Vec::new();
    for reply in replies {
        blocks.extend(collect(reply, deadline).await);
    }
    Ok(Json(MailClaimResponse { blocks }))
}

async fn pending_briefs(persist: PersistHandle, session: Uuid) -> HashMap<Uuid, QueuedLaunchBrief> {
    match persist.claimable_launch_briefs(session).await {
        Ok(briefs) => briefs.into_iter().map(|b| (b.message_uuid, b)).collect(),
        Err(e) => {
            tracing::warn!(session_id = %session, error = %e, "mail claim: reading launch briefs failed");
            HashMap::new()
        }
    }
}

async fn split_queue(state: &AppState, id: &str) -> Vec<(Uuid, SystemReminder)> {
    let mut store = state.store.write().await;
    let claimed = store.claim_reminders(id);
    let mut mailbox = Vec::new();
    let mut rest = Vec::new();
    for reminder in claimed {
        match reminder.source {
            ReminderSource::SessionMailbox { message_uuid, .. } => {
                mailbox.push((message_uuid, reminder));
            }
            _ => rest.push(reminder),
        }
    }
    for reminder in rest.into_iter().rev() {
        store.requeue_reminder(id, reminder);
    }
    mailbox
}

async fn collect(mut reply: oneshot::Receiver<MailBlock>, deadline: Instant) -> Option<MailBlock> {
    match tokio::time::timeout_at(deadline, &mut reply).await {
        Ok(block) => block.ok(),
        Err(_) => {
            reply.close();
            reply.try_recv().ok()
        }
    }
}

fn spawn_claim(
    job: ClaimJob,
    reminder: impl Future<Output = SystemReminder> + Send + 'static,
) -> oneshot::Receiver<MailBlock> {
    let (reply, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let reminder = reminder.await;
        let ReminderSource::SessionMailbox { message_uuid, .. } = reminder.source else {
            return;
        };
        let Some(block) = claim_block(&job, message_uuid, &reminder).await else {
            return;
        };
        if reply.send(block).is_ok() {
            stamp_sent(&job, message_uuid).await;
        } else {
            release_late_claim(&job, message_uuid, reminder).await;
        }
    });
    receiver
}

async fn claim_block(
    job: &ClaimJob,
    message_uuid: Uuid,
    reminder: &SystemReminder,
) -> Option<MailBlock> {
    let id = job.session.to_string();
    match job.persist.launch().claim_message(message_uuid).await {
        Ok(true) => Some(MailBlock {
            source: job.source,
            message_id: Some(message_uuid),
            text: unwrapped(&reminder.payload).to_owned(),
        }),
        Ok(false) => None,
        Err(e) => {
            tracing::warn!(session_id = %id, %message_uuid, error = %e, "mail claim failed — requeueing");
            job.state
                .store
                .write()
                .await
                .requeue_reminder(&id, reminder.clone());
            None
        }
    }
}

fn unwrapped(payload: &str) -> &str {
    payload
        .strip_prefix("<system-reminder>\n")
        .and_then(|rest| rest.strip_suffix("\n</system-reminder>"))
        .unwrap_or(payload)
}

async fn stamp_sent(job: &ClaimJob, message_uuid: Uuid) {
    if let Err(e) = job
        .persist
        .launch()
        .consume_client_claimed(message_uuid)
        .await
    {
        tracing::warn!(session_id = %job.session, %message_uuid, error = %e, "mail claim: recording the message consumed by the client failed");
    }
}

async fn release_late_claim(job: &ClaimJob, message_uuid: Uuid, reminder: SystemReminder) {
    tracing::info!(session_id = %job.session, %message_uuid, "mail claim: answered before this message was claimed — released for the next claim");
    unclaim(job, message_uuid).await;
    job.state
        .store
        .write()
        .await
        .requeue_reminder(&job.session.to_string(), reminder);
}

async fn unclaim(job: &ClaimJob, message_uuid: Uuid) {
    if let Err(e) = job.persist.launch().unclaim_message(message_uuid).await {
        tracing::warn!(session_id = %job.session, %message_uuid, error = %e, "mail claim: releasing a late claim failed");
    }
}

#[cfg(test)]
mod tests;
