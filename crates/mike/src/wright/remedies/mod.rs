use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use serde::Serialize;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;

pub(crate) mod dispatch;
pub(crate) mod filter_rule;
pub(crate) mod fleet;
pub(crate) mod mail;
pub(crate) mod owners;
pub(crate) mod runner;
pub(crate) mod setting;
pub(crate) mod undo;

#[cfg(test)]
mod fixtures_tests;
#[cfg(test)]
mod fleet_tests;
#[cfg(test)]
mod measure_tests;
#[cfg(test)]
mod runner_tests;
#[cfg(test)]
mod undo_tests;
#[cfg(test)]
mod wake_tests;

pub(crate) use owners::{Halt, Owners};
pub(crate) use runner::{PassReport, pass};

const LEASE_ROLE: &str = "wright_remedies";
const TOGGLE_POLL: Duration = Duration::from_secs(15);
const MAX_GAP: Duration = Duration::from_secs(5 * 60);
pub(crate) const PAUSE_AFTER_REFUSALS: i32 = 3;
pub(crate) const TURN_OFF_AFTER_INEFFECTIVE: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    NotifyUndeliveredSender,
    SetSetting,
    DisableFilterRule,
}

impl Action {
    pub(crate) const ALL: [Action; 3] = [
        Action::NotifyUndeliveredSender,
        Action::SetSetting,
        Action::DisableFilterRule,
    ];

    pub(crate) fn code(self) -> &'static str {
        match self {
            Action::NotifyUndeliveredSender => "notify_undelivered_sender",
            Action::SetSetting => "set_setting",
            Action::DisableFilterRule => "disable_filter_rule",
        }
    }

    pub(crate) fn from_code(code: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|action| action.code() == code)
    }

    pub(crate) fn can_undo(self) -> bool {
        matches!(self, Action::SetSetting | Action::DisableFilterRule)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Wake {
    last_run: Option<i64>,
    last_pass: Option<Instant>,
}

impl Wake {
    pub(crate) fn due(&mut self, latest_run: Option<i64>, now: Instant) -> bool {
        let fresh = latest_run.is_some() && latest_run != self.last_run;
        let idle = self
            .last_pass
            .is_none_or(|last| now.duration_since(last) >= MAX_GAP);
        if !fresh && !idle {
            return false;
        }
        self.last_run = latest_run;
        self.last_pass = Some(now);
        true
    }
}

fn log_not_spawned(reason: &str) {
    tracing::debug!(reason, "wright: remedies worker not spawned");
}

fn log_pass(report: &PassReport) {
    log_turned_off(report);
    log_totals(report);
}

fn log_turned_off(report: &PassReport) {
    for measured in report.measured.iter().filter(|m| m.turned_off) {
        tracing::warn!(
            remedy = %measured.remedy,
            ineffective_runs = measured.ineffective_runs,
            "wright: a known fix did not move its measure and was turned off"
        );
    }
}

fn log_totals(report: &PassReport) {
    tracing::info!(
        verified = report.verified,
        refused = report.refused,
        failed = report.failed,
        deferred = report.deferred,
        paused = report.paused,
        measured = report.measured.len(),
        "wright: remedies pass"
    );
}

pub fn spawn_remedies_worker(state: &AppState) {
    let Some(owners) = Owners::from_state(state) else {
        return log_not_spawned("no persistence");
    };
    let toggles = state.feature_toggle_overlay.clone();
    state
        .tracker
        .spawn(run_loop(owners, toggles, state.shutdown()));
    tracing::info!(
        poll_secs = TOGGLE_POLL.as_secs(),
        "wright: remedies worker spawned"
    );
}

async fn run_loop(
    owners: Owners,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    shutdown: CancellationToken,
) {
    let mut wake = Wake::default();
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(TOGGLE_POLL) => {}
        }
        if toggles.load().wright {
            tick(&owners, &mut wake).await;
        }
    }
}

async fn tick(owners: &Owners, wake: &mut Wake) {
    let latest = match owners.persist.web().latest_wright_check_run().await {
        Ok(latest) => latest,
        Err(e) => return log_latest_failed(&e),
    };
    if !wake.due(latest, Instant::now()) {
        return;
    }
    if !owners.persist.holds_worker_role(LEASE_ROLE, MAX_GAP).await {
        return;
    }
    run_pass(owners).await;
}

async fn run_pass(owners: &Owners) {
    match pass(owners).await {
        Ok(report) => log_pass(&report),
        Err(e) => log_pass_failed(&e),
    }
}

fn log_latest_failed(error: &impl std::fmt::Display) {
    tracing::warn!(error = %error, "wright: cannot read the latest check run");
}

fn log_pass_failed(error: &impl std::fmt::Display) {
    tracing::warn!(error = %error, "wright: remedies pass failed");
}
