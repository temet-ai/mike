use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::home;

const RENEW_TIMEOUT: Duration = Duration::from_secs(90);
const RENEW_PROMPT: &str = "Reply with the word ok.";

async fn renew(claude: &Path) -> Result<String, String> {
    let call = tokio::process::Command::new(claude)
        .args(["-p", RENEW_PROMPT, "--model", "haiku"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(RENEW_TIMEOUT, call).await {
        Ok(Ok(status)) => Ok(status.to_string()),
        Ok(Err(e)) => Err(format!("{} could not start: {e}", claude.display())),
        Err(_) => Err(format!("no answer within {} s", RENEW_TIMEOUT.as_secs())),
    }
}

fn is_short(credentials: &Path, deadline: DateTime<Utc>) -> bool {
    std::fs::read(credentials).is_ok_and(|raw| home::login_is_short(&raw, deadline))
}

pub(crate) async fn renew_if_short(claude: &Path, credentials: &Path, deadline: DateTime<Utc>) {
    if !is_short(credentials, deadline) {
        return;
    }
    let outcome = renew(claude).await;
    let still_short = is_short(credentials, deadline);
    tracing::warn!(
        ?outcome,
        still_short,
        "wright: the Claude login expires before the run ends; asked Claude Code to renew it"
    );
}

pub(crate) const RENEW_EVERY: Duration = Duration::from_secs(1800);

pub(crate) struct Gate {
    pub(crate) last_renew: Option<Instant>,
}

pub(crate) fn short_for(credentials: &Path, deadline: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let raw = std::fs::read(credentials).ok()?;
    home::login_expiry(&raw).filter(|_| home::login_is_short(&raw, deadline))
}

impl Gate {
    pub(crate) fn renew_due(&self, now: Instant) -> bool {
        self.last_renew
            .is_none_or(|at| now.duration_since(at) >= RENEW_EVERY)
    }

    pub(crate) async fn refusal(
        &mut self,
        claude: &Path,
        credentials: &Path,
        deadline: DateTime<Utc>,
    ) -> Option<String> {
        short_for(credentials, deadline)?;
        if self.renew_due(Instant::now()) {
            self.last_renew = Some(Instant::now());
            renew_if_short(claude, credentials, deadline).await;
        }
        let expiry = short_for(credentials, deadline)?;
        Some(format!(
            "the Claude login on this machine expires at {expiry}, before a run started now could end; Wright leaves the engineer to another machine until someone runs `claude` here and logs in"
        ))
    }
}
