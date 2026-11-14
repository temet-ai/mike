use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;
use serde_json::Value;

use super::cli::ledger::Ledger;
use super::{LaunchError, SplitLaunchArgs};
use crate::store::persistence::launch::LaunchSurfaceRefs;

pub const PROFILE_ENV: &str = "MIKE_MCP_PROFILE";
pub const TEMPLATE_SEAT_PREFIX: &str = "template-";
pub const BRIEF_LEAD: &str =
    "Your brief follows, copied from mike session notes. No brief file was written for it.";

pub const KINDS_WITH_OWN_PROFILE: [&str; 4] =
    ["coordinator", "acceptance", "observer", "reference"];

pub fn profile_for(seat: &str) -> &'static str {
    KINDS_WITH_OWN_PROFILE
        .into_iter()
        .find(|kind| {
            *kind == seat
                || seat.strip_prefix(*kind).is_some_and(|rest| {
                    rest.strip_prefix('-').is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
                    })
                })
        })
        .unwrap_or("worker")
}

pub fn brief_is_delivered(prompt: &str) -> bool {
    prompt.starts_with(BRIEF_LEAD)
}

#[derive(Debug, Deserialize)]
pub struct RecordedSettings {
    pub run_id: Option<String>,
    pub definition: Value,
}

pub type SeatFuture<'a> = Pin<Box<dyn Future<Output = Result<(), LaunchError>> + Send + 'a>>;

pub type AdmitFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<Box<dyn SeatAdmission>>, LaunchError>> + Send + 'a>>;

pub trait SeatAdmission: Send + Sync {
    fn record_opened<'a>(
        self: Box<Self>,
        launch: i64,
        refs: &'a LaunchSurfaceRefs,
    ) -> SeatFuture<'a>;
}

pub trait SeatLaunchHook: Send + Sync {
    fn before_open<'a>(
        &'a self,
        ledger: &'a Ledger,
        split: &'a mut SplitLaunchArgs,
    ) -> SeatFuture<'a>;

    fn admit<'a>(&'a self, ledger: &'a Ledger, run: &'a str) -> AdmitFuture<'a>;
}

static HOOK: OnceLock<Arc<dyn SeatLaunchHook>> = OnceLock::new();

pub fn install_hook(hook: Arc<dyn SeatLaunchHook>) {
    let _ = HOOK.set(hook);
}

pub async fn before_open(ledger: &Ledger, split: &mut SplitLaunchArgs) -> Result<(), LaunchError> {
    match HOOK.get() {
        Some(hook) => hook.before_open(ledger, split).await,
        None => Ok(()),
    }
}

pub async fn admit(
    ledger: &Ledger,
    run: Option<&str>,
) -> Result<Option<Box<dyn SeatAdmission>>, LaunchError> {
    let Some(run) = run else {
        return Ok(None);
    };
    match HOOK.get() {
        Some(hook) => hook.admit(ledger, run).await,
        None if run.starts_with("fleet-") => Err(LaunchError::Refused(format!(
            "fleet run {run} cannot be admitted: this process has no fleet support installed"
        ))),
        None => Ok(None),
    }
}
