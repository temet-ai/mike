use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::delivery::PROMPT_WAIT_MS;
use super::ledger::Ledger;
use super::runner::Runner;
use crate::launch::LaunchOutput;

const CAPTURE_WAIT: Duration = Duration::from_secs(25);
const CAPTURE_POLL: Duration = Duration::from_millis(500);
const PREFIX_CHARS: usize = 60;
const CLOCK_SLACK: chrono::TimeDelta = chrono::TimeDelta::seconds(2);

pub(super) struct TurnWatch {
    pub(super) session: Uuid,
    pub(super) since: DateTime<Utc>,
    pub(super) prefix: String,
}

impl TurnWatch {
    pub(super) fn new(session: Option<Uuid>, typed: Option<&str>) -> Option<Self> {
        Some(Self {
            session: session?,
            since: Utc::now() - CLOCK_SLACK,
            prefix: prefix(typed?)?,
        })
    }
}

pub(super) fn prefix(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.chars().take(PREFIX_CHARS).collect())
}

pub(super) async fn confirm_from_capture(
    ledger: &Ledger,
    watch: Option<&TurnWatch>,
    captured: bool,
    output: &mut LaunchOutput,
) {
    let Some(watch) = watch else { return };
    if output.confirmed || output.delivered != Some(true) || !captured || !ledger.available() {
        return;
    }
    if wait_for_turn(ledger, watch, CAPTURE_WAIT, CAPTURE_POLL).await {
        output.confirmed = true;
    }
}

pub(super) struct Typed<'a> {
    pub(super) agent: &'a str,
    pub(super) prompt: Option<&'a str>,
    pub(super) captured: bool,
}

pub(super) async fn resend_if_lost(
    runner: &Runner<'_>,
    watch: Option<&TurnWatch>,
    typed: Typed<'_>,
    output: &mut LaunchOutput,
) {
    let (Some(watch), Some(prompt)) = (watch, typed.prompt) else {
        return;
    };
    let lost = !output.confirmed
        && output.delivered == Some(true)
        && typed.captured
        && runner.ledger.available();
    if !lost {
        return;
    }
    eprintln!(
        "mike spawn: the first prompt was not seen to become a turn in {} seconds; sending it again",
        CAPTURE_WAIT.as_secs()
    );
    match runner
        .backend
        .send_prompt(typed.agent, prompt, PROMPT_WAIT_MS)
        .await
    {
        Ok(true) => output.confirmed = true,
        Ok(false) => {
            output.confirmed =
                wait_for_turn(&runner.ledger, watch, CAPTURE_WAIT, CAPTURE_POLL).await;
        }
        Err(e) => eprintln!("mike spawn: the prompt could not be sent again: {e}"),
    }
}

pub(super) async fn wait_for_turn(
    ledger: &Ledger,
    watch: &TurnWatch,
    wait: Duration,
    poll: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if ledger
            .prompt_turn_seen(watch.session, watch.since, &watch.prefix)
            .await
        {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(poll).await;
    }
}
