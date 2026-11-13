use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;
use serde_json::Value;

use super::cli::ledger::Ledger;
use super::{LaunchError, SplitLaunchArgs};

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

pub trait SeatLaunchHook: Send + Sync {
    fn before_open<'a>(
        &'a self,
        ledger: &'a Ledger,
        split: &'a mut SplitLaunchArgs,
    ) -> SeatFuture<'a>;
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
