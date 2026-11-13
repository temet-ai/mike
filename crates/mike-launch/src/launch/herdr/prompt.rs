pub mod blocked;
pub mod rescue;

use std::time::Duration;

use serde_json::Value;

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv, str_field};
use crate::launch::error::LaunchError;

const CALL_MARGIN: Duration = Duration::from_secs(15);

const CONFIRMING_STATES: [&str; 4] = ["working", "done", "idle", "blocked"];

#[derive(Debug, Clone, Copy)]
pub struct Pacing {
    pub poll: Duration,
    pub block_polls: u32,
    pub settle_polls: u32,
    pub rounds: u32,
}

impl Pacing {
    pub const LIVE: Self = Self {
        poll: Duration::from_millis(1000),
        block_polls: 20,
        settle_polls: 4,
        rounds: 3,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOutcome {
    pub delivered: bool,
    pub confirmed: bool,
    pub agent_state: Option<String>,
    pub detail: Option<String>,
}

impl PromptOutcome {
    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }

    fn with_detail_note(mut self, note: &str) -> Self {
        self.detail = Some(match self.detail.take() {
            Some(detail) => format!("{detail}; {note}"),
            None => note.to_string(),
        });
        self
    }
}

pub async fn deliver(
    cli: &HerdrCli,
    name: &str,
    text: &str,
    wait_ms: u64,
) -> Result<PromptOutcome, LaunchError> {
    deliver_paced(cli, name, text, wait_ms, Pacing::LIVE).await
}

pub async fn deliver_paced(
    cli: &HerdrCli,
    name: &str,
    text: &str,
    wait_ms: u64,
    pacing: Pacing,
) -> Result<PromptOutcome, LaunchError> {
    let first = submit(cli, name, text, wait_ms).await?;
    if first.is_confirmed() {
        return Ok(first);
    }
    if first.delivered {
        return Ok(rescue::after_stall(cli, name, text, pacing, first).await);
    }
    match blocked::wait_until_clear(cli, name, pacing).await {
        blocked::Blocked::Cleared => {
            let again = submit(cli, name, text, wait_ms).await?;
            if again.is_confirmed() || !again.delivered {
                return Ok(again);
            }
            Ok(rescue::after_stall(cli, name, text, pacing, again).await)
        }
        blocked::Blocked::Stays(dialog) => Ok(first.with_detail_note(&dialog)),
    }
}

async fn submit(
    cli: &HerdrCli,
    name: &str,
    text: &str,
    wait_ms: u64,
) -> Result<PromptOutcome, LaunchError> {
    let wait = wait_ms.to_string();
    let args = argv(&[
        "agent",
        "prompt",
        name,
        text,
        "--wait",
        "--until",
        "working",
        "--until",
        "blocked",
        "--timeout",
        &wait,
    ]);
    let budget = Duration::from_millis(wait_ms) + CALL_MARGIN;
    match cli.run_json(&args, budget).await {
        Ok(result) => Ok(from_prompted(&result)),
        Err(err) => match err.backend_code() {
            Some("agent_prompt_stalled" | "timeout") => Ok(PromptOutcome {
                delivered: true,
                confirmed: false,
                agent_state: None,
                detail: Some(err.to_string()),
            }),
            Some("agent_blocked") => Ok(PromptOutcome {
                delivered: false,
                confirmed: false,
                agent_state: Some("blocked".to_string()),
                detail: Some(err.to_string()),
            }),
            _ => Err(err),
        },
    }
}

fn from_prompted(result: &Value) -> PromptOutcome {
    let agent = result.get("agent").unwrap_or(&Value::Null);
    let state = str_field(agent, "agent_status").map(str::to_string);
    let confirmed = state
        .as_deref()
        .is_some_and(|s| CONFIRMING_STATES.contains(&s));
    PromptOutcome {
        delivered: true,
        confirmed,
        detail: (!confirmed).then(|| {
            format!(
                "herdr accepted the prompt but reported agent state {}",
                state.as_deref().unwrap_or("(none)")
            )
        }),
        agent_state: state,
    }
}

async fn agent_entry(cli: &HerdrCli, name: &str) -> Option<Value> {
    let result = cli
        .run_json(&argv(&["agent", "get", name]), DEFAULT_TIMEOUT)
        .await
        .ok()?;
    result.get("agent").cloned()
}

async fn agent_status(cli: &HerdrCli, name: &str) -> Option<String> {
    let entry = agent_entry(cli, name).await?;
    str_field(&entry, "agent_status").map(str::to_string)
}

async fn read_screen(cli: &HerdrCli, name: &str) -> Option<String> {
    let args = argv(&[
        "agent", "read", name, "--source", "visible", "--lines", "60",
    ]);
    cli.run_text(&args, DEFAULT_TIMEOUT).await.ok()
}
