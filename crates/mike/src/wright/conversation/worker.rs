use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::engineer::EngineerAnswers;
use super::job::{self, Processed};
use super::steer_apply::Steers;
use super::{Conversation, notice, steering, token};
use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::PersistError;

const LEASE_ROLE: &str = "wright_conversation";
pub(crate) const TOGGLE_POLL: Duration = Duration::from_secs(15);
pub(crate) const IDLE_POLL: Duration = Duration::from_secs(5);

static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);

pub(crate) fn wake() {
    WAKE.notify_one();
}

fn ensure_operator_token() {
    match token::default_path() {
        Ok(path) => write_operator_token(&path),
        Err(reason) => log_no_token_path(&reason),
    }
}

fn write_operator_token(path: &Path) {
    match token::ensure(path) {
        Ok(_) => log_token_ready(path),
        Err(e) => log_token_unwritable(&e, path),
    }
}

fn log_no_token_path(reason: &str) {
    tracing::warn!(reason = %reason, "wright: no operator token written");
}

fn log_token_ready(path: &Path) {
    tracing::info!(path = %path.display(), "wright: operator token ready");
}

fn log_token_unwritable(error: &std::io::Error, path: &Path) {
    tracing::warn!(error = %error, path = %path.display(), "wright: cannot write the operator token");
}

pub fn spawn_conversation_worker(state: &AppState) {
    ensure_operator_token();
    let Some(persist) = state.persist().cloned() else {
        return tracing::debug!("wright: conversation worker not spawned: no persistence");
    };
    let convo = Conversation {
        steers: Arc::new(Steers {
            persist: persist.clone(),
        }),
        engine: Arc::new(EngineerAnswers::for_state(state)),
        persist,
    };
    let toggles = state.feature_toggle_overlay.clone();
    state.tracker.spawn(run(convo, toggles, state.shutdown()));
    tracing::info!("wright: conversation worker spawned");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tick {
    pub(crate) steers_applied: usize,
    pub(crate) processed: Processed,
}

pub(crate) async fn tick(convo: &Conversation) -> Result<Tick, PersistError> {
    let mut steers_applied = steering::apply_pending(convo).await?;
    let processed = job::process_next(convo).await?;
    if matches!(processed, Processed::Answered(_)) {
        steers_applied += steering::apply_pending(convo).await?;
    }
    notice::send_due(&convo.persist).await?;
    Ok(Tick {
        steers_applied,
        processed,
    })
}

async fn step(convo: &Conversation) -> Duration {
    if !convo.engine.available().await
        || !convo.persist.holds_worker_role(LEASE_ROLE, IDLE_POLL).await
    {
        return IDLE_POLL;
    }
    match tick(convo).await {
        Ok(Tick {
            processed: Processed::Idle,
            ..
        }) => IDLE_POLL,
        Ok(_) => Duration::ZERO,
        Err(e) => {
            tracing::warn!(error = %e, "wright: a conversation pass failed");
            IDLE_POLL
        }
    }
}

async fn run(
    convo: Conversation,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    shutdown: CancellationToken,
) {
    loop {
        let wait = if toggles.load().wright {
            step(&convo).await
        } else {
            TOGGLE_POLL
        };
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
            () = WAKE.notified() => {}
        }
    }
}
