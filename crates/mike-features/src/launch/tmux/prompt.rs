use std::time::{Duration, Instant};

use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};
use crate::launch::error::LaunchError;

const PASTE_RENDER_TIMEOUT: Duration = Duration::from_secs(2);
const PASTE_RENDER_POLL: Duration = Duration::from_millis(100);
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(45);
const NO_BOX_TIMEOUT: Duration = Duration::from_secs(5);
const SUBMIT_POLL: Duration = Duration::from_millis(500);
const BOX_MARK: char = '\u{276f}';
const COLLAPSED_PASTE: &str = "[Pasted text";
const PROMPT_LEAD_CHARS: usize = 24;

pub struct PromptOutcome {
    pub delivered: bool,
    pub confirmed: bool,
    pub detail: Option<String>,
}

pub async fn deliver(cli: &TmuxCli, pane: &str, text: &str) -> Result<PromptOutcome, LaunchError> {
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
        Ok(()) => {
            press_until_taken(cli, pane, text, &submit).await;
            Ok(PromptOutcome {
                delivered: true,
                confirmed: false,
                detail: Some(
                    "tmux has no agent_state capability; the channel check is the pass signal, \
                     not this field"
                        .to_string(),
                ),
            })
        }
        Err(e) => Ok(PromptOutcome {
            delivered: true,
            confirmed: false,
            detail: Some(format!("pasted but the submitting Enter failed: {e}")),
        }),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoxState {
    NotDrawn,
    Unsent,
    Taken,
}

pub fn box_state(screen: &str, text: &str) -> BoxState {
    let Some(line) = screen
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with(BOX_MARK))
    else {
        return BoxState::NotDrawn;
    };
    let typed = line.trim_start_matches(|c: char| c == BOX_MARK || c.is_whitespace());
    let lead: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(PROMPT_LEAD_CHARS)
        .collect();
    if typed.starts_with(COLLAPSED_PASTE) || (!lead.is_empty() && typed.starts_with(&lead)) {
        BoxState::Unsent
    } else {
        BoxState::Taken
    }
}

async fn press_until_taken(cli: &TmuxCli, pane: &str, text: &str, submit: &[String]) {
    let started = Instant::now();
    let mut box_seen = false;
    while started.elapsed() < SUBMIT_TIMEOUT {
        tokio::time::sleep(SUBMIT_POLL).await;
        let state = capture_pane(cli, pane).await.map(|s| box_state(&s, text));
        box_seen |= matches!(state, Some(BoxState::Unsent | BoxState::Taken));
        if !box_seen && started.elapsed() >= NO_BOX_TIMEOUT {
            return;
        }
        match state {
            Some(BoxState::Unsent) => {
                if let Err(e) = cli.run_ok(submit, DEFAULT_TIMEOUT).await {
                    eprintln!("mike spawn: pressing Enter in {pane} failed again: {e}");
                    return;
                }
            }
            Some(BoxState::Taken) | None => return,
            Some(BoxState::NotDrawn) => {}
        }
    }
    eprintln!("mike spawn: the prompt in {pane} was still unsent after {SUBMIT_TIMEOUT:?}");
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
