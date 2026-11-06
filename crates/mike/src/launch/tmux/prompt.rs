use std::time::{Duration, Instant};

use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};
use crate::launch::error::LaunchError;

const PASTE_RENDER_TIMEOUT: Duration = Duration::from_secs(2);
const PASTE_RENDER_POLL: Duration = Duration::from_millis(100);

pub(super) struct PromptOutcome {
    pub(super) delivered: bool,
    pub(super) confirmed: bool,
    pub(super) detail: Option<String>,
}

pub(super) async fn deliver(
    cli: &TmuxCli,
    pane: &str,
    text: &str,
) -> Result<PromptOutcome, LaunchError> {
    let baseline = capture_pane(cli, pane).await;
    let buffer = format!("mike-prompt-{pane}").replace(['%', ':'], "_");
    let set = argv(&["set-buffer", "-b", &buffer, "--", text]);
    if let Err(e) = cli.run_ok(&set, DEFAULT_TIMEOUT).await {
        return Ok(PromptOutcome {
            delivered: false,
            confirmed: false,
            detail: Some(e.to_string()),
        });
    }
    let paste = argv(&["paste-buffer", "-p", "-d", "-b", &buffer, "-t", pane]);
    if let Err(e) = cli.run_ok(&paste, DEFAULT_TIMEOUT).await {
        return Ok(PromptOutcome {
            delivered: false,
            confirmed: false,
            detail: Some(e.to_string()),
        });
    }
    wait_for_render(cli, pane, baseline.as_deref()).await;
    let submit = argv(&["send-keys", "-t", pane, "Enter"]);
    match cli.run_ok(&submit, DEFAULT_TIMEOUT).await {
        Ok(()) => Ok(PromptOutcome {
            delivered: true,
            confirmed: false,
            detail: Some(
                "tmux has no agent_state capability; the channel check is the pass signal, \
                 not this field"
                    .to_string(),
            ),
        }),
        Err(e) => Ok(PromptOutcome {
            delivered: true,
            confirmed: false,
            detail: Some(format!("pasted but the submitting Enter failed: {e}")),
        }),
    }
}

async fn capture_pane(cli: &TmuxCli, pane: &str) -> Option<String> {
    let args = argv(&["capture-pane", "-p", "-t", pane]);
    cli.run_text(&args, DEFAULT_TIMEOUT).await.ok()
}

async fn wait_for_render(cli: &TmuxCli, pane: &str, baseline: Option<&str>) {
    let deadline = Instant::now() + PASTE_RENDER_TIMEOUT;
    loop {
        let current = capture_pane(cli, pane).await;
        if current.as_deref() != baseline {
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(PASTE_RENDER_POLL).await;
    }
}
