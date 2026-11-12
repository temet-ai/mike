use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use uuid::Uuid;

use super::claude::ClaudeRecipient;
use super::codex::{CodexRecipient, LiveSeat};
use super::deliver::{Outcome, deliver};
use super::mailbox_link::{mailbox_state, mirror_codex_mail};
use super::store::{self, Mail};
use super::{Recipient, Runtime, Stage};
use crate::config::AppState;
use crate::store::persistence::{PersistError, PersistHandle};

const SWEEP_INTERVAL: Duration = Duration::from_secs(5);
const MIRROR_BATCH: i64 = 200;
const DELIVER_BATCH: i64 = 100;

pub(crate) fn spawn_mail_outbox(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("mail_outbox: no persistence, not spawning");
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        let mut ticker = interval(SWEEP_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    if let Err(error) = sweep(&persist).await {
                        tracing::warn!(error = %error, "mail_outbox: sweep failed");
                    }
                }
            }
        }
    });
}

pub(crate) async fn sweep(persist: &PersistHandle) -> Result<Vec<(Uuid, Outcome)>, PersistError> {
    let pool = persist.pool();
    mirror_codex_mail(pool, MIRROR_BATCH).await?;
    persist.reconcile_runtime_mail_outbox().await?;
    let mut outcomes = Vec::new();
    let mut queue_jobs = tokio::task::JoinSet::new();
    for message in store::open_messages(pool, DELIVER_BATCH).await? {
        if let Some(outcome) = settle_message(persist, message, &mut queue_jobs).await? {
            outcomes.push((message, outcome));
        }
    }
    drain_queue_jobs(&mut queue_jobs).await;
    Ok(outcomes)
}

async fn settle_message(
    persist: &PersistHandle,
    message: Uuid,
    queue_jobs: &mut tokio::task::JoinSet<()>,
) -> Result<Option<Outcome>, PersistError> {
    let pool = persist.pool();
    let Some(mail) = store::load(pool, message).await? else {
        return Ok(None);
    };
    if mail.runtime == Some(Runtime::Codex) && !crate::launch::codex::live_seat(mail.to) {
        return Ok(handle_external(persist, &mail, queue_jobs).await?);
    }
    let Some(recipient) = recipient_for(persist, &mail).await else {
        return Ok(None);
    };
    let outcome = deliver(pool, recipient.as_ref(), message).await?;
    log_settled(pool, message, &outcome).await?;
    Ok(Some(outcome))
}

async fn drain_queue_jobs(queue_jobs: &mut tokio::task::JoinSet<()>) {
    while let Some(result) = queue_jobs.join_next().await {
        if let Err(error) = result {
            tracing::warn!(error = %error, "Codex mailbox notification task failed");
        }
    }
}

async fn handle_external(
    persist: &PersistHandle,
    mail: &Mail,
    jobs: &mut tokio::task::JoinSet<()>,
) -> Result<Option<Outcome>, sqlx::Error> {
    if let Some(outcome) = reconcile_external(persist.pool(), mail.message).await? {
        return Ok(Some(outcome));
    }
    if jobs.len() < 4
        && let Some(job) = super::codex_queue::prepare(persist, mail.to, mail.message).await
    {
        jobs.spawn(job.run(persist.clone()));
    }
    Ok(None)
}

async fn reconcile_external(
    pool: &sqlx::PgPool,
    message: Uuid,
) -> Result<Option<Outcome>, sqlx::Error> {
    let Some(state) = mailbox_state(pool, message).await? else {
        return Ok(None);
    };
    if state.consumed_at.is_some() {
        store::record_stage(
            pool,
            message,
            Stage::Consumed,
            None,
            "recipient acknowledged mailbox message",
        )
        .await?;
        store::settle(
            pool,
            message,
            "delivered",
            Some("recipient acknowledged mailbox message"),
        )
        .await?;
        return Ok(Some(Outcome::AlreadySettled("delivered".into())));
    }
    if state.dead_lettered_at.is_some() {
        store::settle(pool, message, "failed", Some("mailbox dead letter")).await?;
        return Ok(Some(Outcome::AlreadySettled("failed".into())));
    }
    Ok(None)
}

async fn log_settled(
    pool: &sqlx::PgPool,
    message: Uuid,
    outcome: &Outcome,
) -> Result<(), sqlx::Error> {
    if matches!(outcome, Outcome::Delivered(_) | Outcome::Unresolved(_)) {
        let evidence = store::stages(pool, message).await?;
        tracing::info!(message_id = %message, outcome = ?outcome, stages = ?evidence, "mail_outbox: settled");
    }
    Ok(())
}

async fn recipient_for(persist: &PersistHandle, mail: &Mail) -> Option<Box<dyn Recipient>> {
    match mail.runtime? {
        Runtime::Claude => Some(Box::new(ClaudeRecipient {
            persist: persist.clone(),
            session: mail.to,
        })),
        Runtime::Codex => {
            let binding = persist.runtime_thread_by_session(mail.to).await.ok()??;
            Some(Box::new(CodexRecipient {
                rpc: LiveSeat {
                    persist: persist.clone(),
                    session: mail.to,
                },
                thread_id: binding.provider.thread_id,
                pool: Some(persist.pool().clone()),
            }))
        }
    }
}
