use std::time::Duration;

use super::cli::{HcCli, argv};
use super::terminal::{self, Terminal};
use crate::launch::error::LaunchError;

const POLL: Duration = Duration::from_millis(500);
const SEND_MARGIN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromptOutcome {
    pub(super) delivered: bool,
    pub(super) confirmed: bool,
    pub(super) agent_state: Option<String>,
    pub(super) detail: Option<String>,
}

pub(super) fn turn_started(before: Option<&Terminal>, now: &Terminal) -> bool {
    let advanced = match (before.and_then(|t| t.turn), now.turn) {
        (Some(b), Some(n)) => n > b,
        _ => false,
    };
    advanced || matches!(now.status.as_deref(), Some("working" | "blocked"))
}

pub(super) async fn deliver(
    cli: &HcCli,
    address: &str,
    text: &str,
    wait_ms: u64,
) -> Result<PromptOutcome, LaunchError> {
    let before = terminal::get(cli, address).await?;
    if before.as_ref().and_then(|t| t.status.as_deref()) == Some("blocked") {
        return Ok(PromptOutcome {
            delivered: false,
            confirmed: false,
            agent_state: Some("blocked".to_string()),
            detail: Some(format!(
                "herdr-core terminal {address} is blocked on a dialog"
            )),
        });
    }
    let wait = Duration::from_millis(wait_ms);
    cli.hc_json(
        &argv(&["send", "--json", address, text]),
        wait + SEND_MARGIN,
    )
    .await?;
    let deadline = tokio::time::Instant::now() + wait;
    let mut last = None;
    loop {
        let now = terminal::get(cli, address).await?;
        if let Some(now) = &now
            && turn_started(before.as_ref(), now)
        {
            return Ok(PromptOutcome {
                delivered: true,
                confirmed: true,
                agent_state: now.status.clone(),
                detail: None,
            });
        }
        last = now.and_then(|t| t.status).or(last);
        if tokio::time::Instant::now() >= deadline {
            return Ok(PromptOutcome {
                delivered: true,
                confirmed: false,
                detail: Some(format!(
                    "herdr-core took the prompt but no turn started within {wait_ms} ms (status {})",
                    last.as_deref().unwrap_or("(none)")
                )),
                agent_state: last,
            });
        }
        tokio::time::sleep(POLL).await;
    }
}

pub(super) async fn deliver_by_name(
    cli: &HcCli,
    name: &str,
    text: &str,
    wait_ms: u64,
) -> Result<bool, LaunchError> {
    let Some(found) = terminal::by_name(cli, name).await? else {
        return Err(LaunchError::Failed(format!(
            "no live herdr-core terminal carries {}={name}",
            terminal::NAME_LABEL
        )));
    };
    Ok(deliver(cli, &found.address, text, wait_ms).await?.confirmed)
}
