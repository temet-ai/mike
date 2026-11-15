use std::time::Duration;

use super::cli::{DEFAULT_TIMEOUT, HcCli, argv};
use super::terminal::{self, Terminal};
use crate::launch::error::LaunchError;
use crate::launch::herdr::prompt::blocked_jev;

const POLL: Duration = Duration::from_millis(500);
const SEND_MARGIN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOutcome {
    pub delivered: bool,
    pub confirmed: bool,
    pub agent_state: Option<String>,
    pub detail: Option<String>,
}

pub fn turn_started(before: Option<&Terminal>, now: &Terminal) -> bool {
    let advanced = match (before.and_then(|t| t.turn), now.turn) {
        (Some(b), Some(n)) => n > b,
        _ => false,
    };
    advanced || matches!(now.status.as_deref(), Some("working" | "blocked"))
}

pub async fn deliver(
    cli: &HcCli,
    address: &str,
    text: &str,
    wait_ms: u64,
) -> Result<PromptOutcome, LaunchError> {
    let before = terminal::get(cli, address).await?;
    if before.as_ref().and_then(|t| t.status.as_deref()) == Some("blocked") {
        let dialog = blocked_dialog(cli, address).await;
        return Ok(PromptOutcome {
            delivered: false,
            confirmed: false,
            agent_state: Some("blocked".to_string()),
            detail: Some(blocked_detail(address, dialog.as_deref())),
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

async fn blocked_dialog(cli: &HcCli, address: &str) -> Option<String> {
    let screen = cli
        .hc_text(&argv(&["read", address]), DEFAULT_TIMEOUT)
        .await
        .ok()
        .filter(|screen| !screen.trim().is_empty())?;
    let decider = crate::llm::decisions::ambient::decider(blocked_jev::POINT);
    blocked_jev::dialog(decider.as_ref(), &screen).await
}

pub fn blocked_detail(address: &str, dialog: Option<&str>) -> String {
    match dialog {
        Some(dialog) => format!("herdr-core terminal {address} is blocked on {dialog}"),
        None => format!("herdr-core terminal {address} is blocked on a dialog"),
    }
}

pub async fn deliver_by_name(
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
