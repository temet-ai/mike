use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;

use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::wright::FollowupLimits;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) mod body;
pub(crate) mod context;
pub(crate) mod file;
pub(crate) mod follow;
pub(crate) mod handoff;
pub(crate) mod prompt;
pub(crate) mod quiet;
pub(crate) mod recurring;
pub(crate) mod runner;
pub(crate) mod sessions;
pub(crate) mod tick;

#[cfg(test)]
mod fakes_tests;
#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod follow_tests;
#[cfg(test)]
mod handoff_tests;
#[cfg(test)]
mod parse_tests;

use context::{FollowupContext, Here, gh_ready};
use runner::{ProcessRunner, StepError};
use sessions::LedgerSessions;

const LEASE_ROLE: &str = "wright_followups";
const TICK: Duration = Duration::from_secs(60);
const TOGGLE_POLL: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub(crate) enum FollowupError {
    #[error(transparent)]
    Persist(#[from] PersistError),
    #[error(transparent)]
    Step(#[from] StepError),
    #[error("{0}")]
    Output(String),
}

type LiveContext = FollowupContext<ProcessRunner, LedgerSessions>;

struct Worker {
    persist: PersistHandle,
    ctx: Option<LiveContext>,
    last_unavailable: Option<String>,
}

fn note_unavailable(last: &mut Option<String>, reason: String) {
    if last.as_deref() != Some(reason.as_str()) {
        tracing::info!(reason = %reason, "wright follow-ups: this machine cannot do the work");
        *last = Some(reason);
    }
}

impl Worker {
    async fn limits(&self) -> Option<FollowupLimits> {
        match self.persist.web().wright_followup_limits().await {
            Ok(limits) => Some(limits),
            Err(e) => {
                tracing::warn!(error = %e, "wright follow-ups: the limits cannot be read");
                None
            }
        }
    }

    async fn ready(&mut self, limits: &FollowupLimits) -> Result<(), String> {
        if self.ctx.is_none() {
            gh_ready(&ProcessRunner).await?;
            self.ctx = Some(FollowupContext {
                persist: self.persist.clone(),
                runner: ProcessRunner,
                sessions: LedgerSessions {
                    persist: self.persist.clone(),
                },
                here: Here::detect().await,
            });
        }
        match &self.ctx {
            Some(ctx) => ctx.here.unreachable(&limits.fixers).map_or(Ok(()), Err),
            None => Err("the follow-up context was not built".to_owned()),
        }
    }

    async fn iteration(&mut self) {
        let Some(limits) = self.limits().await else {
            return;
        };
        if let Err(reason) = self.ready(&limits).await {
            note_unavailable(&mut self.last_unavailable, reason);
            return;
        }
        self.last_unavailable = None;
        let Some(ctx) = self.ctx.as_ref() else {
            return;
        };
        if !self.persist.holds_worker_role(LEASE_ROLE, TICK).await {
            return;
        }
        tick::log_tick(&tick::tick(ctx, &limits).await);
    }
}

async fn run(
    mut worker: Worker,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    shutdown: CancellationToken,
) {
    loop {
        let pause = if toggles.load().wright {
            worker.iteration().await;
            TICK
        } else {
            TOGGLE_POLL
        };
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(pause) => {}
        }
    }
}

pub fn spawn_followups_worker(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        return tracing::debug!("wright follow-ups: not spawning without persistence");
    };
    let worker = Worker {
        persist,
        ctx: None,
        last_unavailable: None,
    };
    let toggles = state.feature_toggle_overlay.clone();
    state.tracker.spawn(run(worker, toggles, state.shutdown()));
    tracing::info!(
        tick_secs = TICK.as_secs(),
        "wright follow-ups: worker spawned; it idles while the wright toggle is off"
    );
}
