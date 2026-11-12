use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use uuid::Uuid;

use super::claude::ClaudeRecipient;
use super::codex::{CodexRecipient, LiveSeat};
use super::deliver::{Outcome, deliver};
use super::mailbox_link::mirror_codex_mail;
use super::store::{self, Mail};
use super::{Recipient, Runtime};
use crate::config::AppState;
use crate::store::persistence::PersistHandle;

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

pub(crate) async fn sweep(persist: &PersistHandle) -> Result<Vec<(Uuid, Outcome)>, sqlx::Error> {
    let pool = persist.pool();
    mirror_codex_mail(pool, MIRROR_BATCH).await?;
    let mut outcomes = Vec::new();
    for message in store::open_messages(pool, DELIVER_BATCH).await? {
        let Some(mail) = store::load(pool, message).await? else {
            continue;
        };
        let Some(recipient) = recipient_for(persist, &mail).await else {
            continue;
        };
        let outcome = deliver(pool, recipient.as_ref(), message).await?;
        if matches!(outcome, Outcome::Delivered(_) | Outcome::Unresolved(_)) {
            let evidence = store::stages(pool, message).await?;
            tracing::info!(message_id = %message, outcome = ?outcome, stages = ?evidence, "mail_outbox: settled");
        }
        outcomes.push((message, outcome));
    }
    Ok(outcomes)
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
