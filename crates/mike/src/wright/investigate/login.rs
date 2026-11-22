use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

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
