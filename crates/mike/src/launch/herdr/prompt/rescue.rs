use super::{Pacing, PromptOutcome, agent_entry, agent_status, read_screen};
use crate::launch::herdr::cli::{DEFAULT_TIMEOUT, HerdrCli, argv, str_field};

const NEEDLE_CHARS: usize = 40;
const PASTED_MARKER: &str = "[Pasted text";

pub(super) async fn after_stall(
    cli: &HerdrCli,
    name: &str,
    text: &str,
    pacing: Pacing,
    stalled: PromptOutcome,
) -> PromptOutcome {
    for round in 0..pacing.rounds {
        let Some(screen) = read_screen(cli, name).await else {
            return stalled;
        };
        if !input_holds(&screen, text) {
            return if round == 0 {
                stalled
            } else {
                stalled.with_detail_note("the typed prompt left the input after Enter")
            };
        }
        if !press_enter(cli, name, round > 0).await {
            return stalled;
        }
        if let Some(state) = settled_state(cli, name, pacing).await {
            return PromptOutcome {
                delivered: true,
                confirmed: true,
                agent_state: Some(state),
                detail: None,
            };
        }
    }
    stalled.with_detail_note(&format!(
        "the typed prompt still sat unsubmitted in the input after {} Enter presses",
        pacing.rounds
    ))
}

async fn press_enter(cli: &HerdrCli, name: &str, with_carriage_return: bool) -> bool {
    let enter = argv(&["agent", "send-keys", name, "enter"]);
    if cli.run_json(&enter, DEFAULT_TIMEOUT).await.is_err() {
        return false;
    }
    if !with_carriage_return {
        return true;
    }
    let Some(entry) = agent_entry(cli, name).await else {
        return false;
    };
    let Some(pane) = str_field(&entry, "pane_id") else {
        return false;
    };
    let text = argv(&["pane", "send-text", pane, "\r"]);
    cli.run_json(&text, DEFAULT_TIMEOUT).await.is_ok()
}

async fn settled_state(cli: &HerdrCli, name: &str, pacing: Pacing) -> Option<String> {
    for _ in 0..pacing.settle_polls {
        tokio::time::sleep(pacing.poll).await;
        if let Some(state) = agent_status(cli, name).await
            && matches!(state.as_str(), "working" | "blocked")
        {
            return Some(state);
        }
    }
    None
}

pub(super) fn input_holds(screen: &str, text: &str) -> bool {
    let region = input_region(screen);
    if region.contains(PASTED_MARKER) {
        return true;
    }
    let needle = needle(text);
    !needle.is_empty() && squeeze(&region).contains(&needle)
}

fn needle(text: &str) -> String {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty());
    first
        .map(|line| squeeze(line).chars().take(NEEDLE_CHARS).collect())
        .unwrap_or_default()
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn input_region(screen: &str) -> String {
    let lines: Vec<&str> = screen.lines().collect();
    let Some(start) = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with(['❯', '>']))
    else {
        return String::new();
    };
    let mut region = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(start) {
        if i > start && is_rule(line) {
            break;
        }
        region.push(line.trim_start_matches(|c: char| c.is_whitespace() || c == '❯' || c == '>'));
    }
    region.join("\n")
}

fn is_rule(line: &str) -> bool {
    line.matches('─').count() >= 3
}
