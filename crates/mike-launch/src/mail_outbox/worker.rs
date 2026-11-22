use crate::store::persistence::runtime_repo::RuntimePersist as _;
use std::time::Duration;

use uuid::Uuid;

use super::claude::ClaudeRecipient;
use super::codex::{CodexRecipient, LiveSeat};
use super::deliver::{Outcome, deliver};
use super::mailbox_link::{codex_bound, mailbox_state, mirror_codex_mail};
use super::store::{self, Mail};
use super::{Recipient, Runtime, Stage};
use crate::base_state::BaseState;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::RowChangedTable;
use crate::store::persistence::{PersistError, PersistHandle};

const SWEEP_INTERVAL: Duration = Duration::from_secs(5);
const IDLE_BACKSTOP: Duration = Duration::from_secs(120);
const WATCHED: [RowChangedTable; 2] = [
    RowChangedTable::MailboxMessages,
    RowChangedTable::RuntimeThreads,
];
const UNBOUND_WATCHED: [RowChangedTable; 1] = [RowChangedTable::RuntimeThreads];
const MIRROR_BATCH: i64 = 200;
const DELIVER_BATCH: i64 = 100;

pub fn spawn_mail_outbox(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("mail_outbox: no persistence, not spawning");
        return;
    };
    let shutdown = state.shutdown();
    let feed = state.change_feed.clone();
    state.tracker.spawn(async move {
        let mut bindings = Bindings::default();
        loop {
            let since = feed.generation();
            let wait = pass(&persist, &feed, &mut bindings, since).await;
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                () = wait_for_mail(&feed, wait, since) => {}
            }
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    Interval,
    Change(&'static [RowChangedTable]),
}

async fn pass(
    persist: &PersistHandle,
    feed: &ChangeFeed,
    bindings: &mut Bindings,
    since: Generation,
) -> Wait {
    match bindings.bound(persist, feed, since).await {
        Ok(true) => after_sweep(sweep(persist).await),
        Ok(false) => Wait::Change(&UNBOUND_WATCHED),
        Err(error) => {
            tracing::warn!(error = %error, "mail_outbox: reading Codex bindings failed");
            Wait::Interval
        }
    }
}

fn after_sweep(swept: Result<bool, PersistError>) -> Wait {
    match swept {
        Ok(false) => Wait::Change(&WATCHED),
        Ok(true) => Wait::Interval,
        Err(error) => {
            tracing::warn!(error = %error, "mail_outbox: sweep failed");
            Wait::Interval
        }
    }
}

#[derive(Default)]
struct Bindings {
    bound: Option<bool>,
    checked: Generation,
}

impl Bindings {
    async fn bound(
        &mut self,
        persist: &PersistHandle,
        feed: &ChangeFeed,
        since: Generation,
    ) -> Result<bool, sqlx::Error> {
        if let Some(bound) = self.bound
            && feed.is_fed()
            && !feed.changed_since(&UNBOUND_WATCHED, self.checked)
        {
            return Ok(bound);
        }
        let bound = codex_bound(persist.pool()).await?;
        self.bound = Some(bound);
        self.checked = since;
        Ok(bound)
    }
}

async fn wait_for_mail(feed: &ChangeFeed, wait: Wait, since: Generation) {
    match wait {
        Wait::Change(tables) if feed.is_fed() => {
            feed.wait_for_change(tables, since, IDLE_BACKSTOP).await;
        }
        Wait::Interval | Wait::Change(_) => tokio::time::sleep(SWEEP_INTERVAL).await,
    }
}

pub async fn sweep(persist: &PersistHandle) -> Result<bool, PersistError> {
    let pool = persist.pool();
    let mirrored = mirror_codex_mail(pool, MIRROR_BATCH).await?;
    let reconciled = persist.runtime().reconcile_runtime_mail_outbox().await?;
    let open = store::open_messages(pool, DELIVER_BATCH).await?;
    let mut queue_jobs = tokio::task::JoinSet::new();
    for message in &open {
        settle_message(persist, *message, &mut queue_jobs).await?;
    }
    drain_queue_jobs(&mut queue_jobs).await;
    Ok(mirrored > 0 || reconciled > 0 || !open.is_empty())
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
            let binding = persist
                .runtime()
                .runtime_thread_by_session(mail.to)
                .await
                .ok()??;
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

#[cfg(test)]
#[path = "worker_wait_tests.rs"]
mod wait_tests;
