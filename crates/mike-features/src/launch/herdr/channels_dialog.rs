use std::time::Duration;

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv};
use crate::launch::error::LaunchError;

const WAIT_TIMEOUT_MS: &str = "30000";
const WAIT_BUDGET: Duration = Duration::from_secs(35);
const DIALOG_TEXT: &str = "Loading development channels";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelsDialogOutcome {
    Confirmed,
    NotSeen,
}

fn dialog_was_seen(wait_result: &Result<serde_json::Value, LaunchError>) -> bool {
    wait_result.is_ok()
}

async fn dialog_appeared(cli: &HerdrCli, pane: &str) -> bool {
    let wait_args = argv(&[
        "pane",
        "wait-output",
        pane,
        "--match",
        DIALOG_TEXT,
        "--timeout",
        WAIT_TIMEOUT_MS,
    ]);
    dialog_was_seen(&cli.run_json(&wait_args, WAIT_BUDGET).await)
}

async fn press_enter(cli: &HerdrCli, pane: &str) -> Result<(), LaunchError> {
    let send_args = argv(&["pane", "send-keys", pane, "enter"]);
    cli.run_json(&send_args, DEFAULT_TIMEOUT).await.map(|_| ())
}

fn log_not_seen(pane: &str) {
    tracing::info!(
        pane,
        "mike spawn: development-channels dialog was not observed within the wait budget, \
         so it was left alone; the session starts without MCP-channel push-wake"
    );
}

fn log_confirmed(pane: &str) {
    tracing::info!(
        pane,
        "mike spawn: confirmed the development-channels dialog"
    );
}

fn log_enter_failed(pane: &str, error: &LaunchError) {
    tracing::warn!(
        pane,
        %error,
        "mike spawn: saw the development-channels dialog but failed to send Enter"
    );
}

pub async fn confirm(cli: &HerdrCli, pane: &str) -> ChannelsDialogOutcome {
    if !dialog_appeared(cli, pane).await {
        log_not_seen(pane);
        return ChannelsDialogOutcome::NotSeen;
    }
    match press_enter(cli, pane).await {
        Ok(()) => {
            log_confirmed(pane);
            ChannelsDialogOutcome::Confirmed
        }
        Err(error) => {
            log_enter_failed(pane, &error);
            ChannelsDialogOutcome::NotSeen
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn dialog_seen_when_wait_output_succeeds() {
        assert!(dialog_was_seen(&Ok(serde_json::json!({}))));
    }

    #[test]
    fn dialog_not_seen_when_wait_output_errors() {
        assert!(!dialog_was_seen(&Err(LaunchError::Timeout {
            command: "herdr pane wait-output".to_string(),
            after: Duration::from_secs(30),
        })));
    }
}
