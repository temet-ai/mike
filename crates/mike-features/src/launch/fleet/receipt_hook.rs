use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

#[cfg(test)]
#[path = "receipt_hook_tests.rs"]
mod tests;

pub const ENABLED_ENV: &str = "MIKE_FLEET_RECEIPTS";
pub const INSPECT_ADDR_ENV: &str = "MIKE_INSPECT_ADDR";
const DEFAULT_INSPECT_ADDR: &str = "127.0.0.1:9989";
const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;
const SLOW_SECONDS: i64 = 5;
const RESULTS_READ: usize = 3;
const RUN_SUFFIX_CHARS: usize = 9;
const HTTP_TIMEOUT: Duration = Duration::from_millis(1500);

pub fn append_hook(settings: &mut Value, executable: &Path) {
    if settings["disableAllHooks"] == true || settings["env"][ENABLED_ENV] == "0" {
        return;
    }
    let Some(fields) = settings.as_object_mut() else {
        return;
    };
    let Some(hooks) = fields
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
    else {
        return;
    };
    let Some(before) = hooks
        .entry("PreToolUse")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return;
    };
    let escaped = executable.to_string_lossy().replace('\'', "'\\''");
    let handler = json!({
        "matcher": "Bash",
        "hooks": [{
            "type": "command",
            "command": format!("'{escaped}' fleet receipt-hook"),
            "timeout": 3,
        }],
    });
    if !before.contains(&handler) {
        before.push(handler);
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Receipt {
    #[serde(default)]
    pub seat: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub completion_unknown: bool,
    #[serde(default)]
    pub still_valid: Option<bool>,
    #[serde(default)]
    pub use_id: Option<i64>,
    #[serde(default)]
    pub changed_since: Vec<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub is_error: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct Results {
    #[serde(default)]
    results: Vec<Receipt>,
}

pub fn slug_of_run(run: &str) -> Option<&str> {
    let rest = run.strip_prefix("fleet-")?;
    let cut = rest.len().checked_sub(RUN_SUFFIX_CHARS)?;
    let (slug, suffix) = rest.split_at(cut);
    let hex = suffix.strip_prefix('-')?;
    (!slug.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(slug)
}

pub fn command_of(event: &Value) -> Option<&str> {
    if event["hook_event_name"] != "PreToolUse" || event["tool_name"] != "Bash" {
        return None;
    }
    event
        .get("tool_input")?
        .get("command")?
        .as_str()
        .map(str::trim)
        .filter(|c| !c.is_empty())
}

fn duration_seconds(receipt: &Receipt) -> Option<i64> {
    Some((receipt.finished_at? - receipt.started_at?).num_seconds())
}

pub fn worth_telling(receipt: &Receipt) -> bool {
    !receipt.running
        && !receipt.completion_unknown
        && receipt.still_valid != Some(false)
        && receipt.changed_since.is_empty()
        && duration_seconds(receipt).is_some_and(|s| s >= SLOW_SECONDS)
}

fn ago_text(minutes: i64) -> String {
    if minutes >= 120 {
        format!("{} h ago", minutes / 60)
    } else {
        format!("{minutes} min ago")
    }
}

pub fn context_for(
    command: &str,
    receipts: &[Receipt],
    now: DateTime<Utc>,
) -> Option<String> {
    let receipt = receipts
        .iter()
        .filter(|r| r.command.as_deref().map(str::trim) == Some(command))
        .filter(|r| worth_telling(r))
        .max_by_key(|r| r.finished_at)?;
    let finished = receipt.finished_at?;
    let ago = ago_text((now - finished).num_minutes().max(0));
    let took = duration_seconds(receipt).unwrap_or_default();
    let outcome = match receipt.is_error {
        Some(true) => "failed",
        _ => "finished",
    };
    let seat = receipt.seat.as_deref().unwrap_or("another seat");
    let where_output = receipt
        .use_id
        .map(|id| format!(" list_fleet_results (use_id {id}) has its output."))
        .unwrap_or_default();
    let validity = match receipt.still_valid {
        Some(true) => "no path it covers has changed since",
        _ => {
            "no checkout change was recorded since it ran, which does not prove its result still holds"
        }
    };
    Some(format!(
        "[mike receipt] `{command}` {outcome} {ago} in seat {seat} after {took}s; {validity}.{where_output} Run it again only when you need a result newer than that one."
    ))
}

fn response(context: Option<String>) -> Value {
    match context {
        Some(text) => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": text,
            }
        }),
        None => json!({}),
    }
}

fn read_event(input: impl Read) -> Result<Value, String> {
    let mut bytes = Vec::new();
    input
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("hook input exceeded 4 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn fetch_receipts(addr: &str, slug: &str, command: &str) -> Result<Vec<Receipt>, String> {
    let mut url = reqwest::Url::parse(&format!("http://{addr}/api/fleets/{slug}/results"))
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("command", command)
        .append_pair("limit", &RESULTS_READ.to_string())
        .append_pair("include_output", "true");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let response = reqwest::Client::new()
            .get(url)
            .timeout(HTTP_TIMEOUT)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("the inspect API answered {}", response.status()));
        }
        let body: Results = response.json().await.map_err(|error| error.to_string())?;
        Ok(body.results)
    })
}

fn process(input: impl Read, mut output: impl Write, run: &str) -> Result<(), String> {
    let event = read_event(input)?;
    let context = command_of(&event).and_then(|command| {
        let slug = slug_of_run(run)?;
        let addr = std::env::var(INSPECT_ADDR_ENV).unwrap_or_else(|_| DEFAULT_INSPECT_ADDR.into());
        let receipts = fetch_receipts(&addr, slug, command).ok()?;
        context_for(command, &receipts, Utc::now())
    });
    serde_json::to_writer(&mut output, &response(context)).map_err(|error| error.to_string())
}

pub fn run() -> i32 {
    let Ok(run) = std::env::var("MIKE_RUN_ID") else {
        return 0;
    };
    if !run.starts_with("fleet-") || std::env::var(ENABLED_ENV).ok().as_deref() == Some("0") {
        return 0;
    }
    match process(std::io::stdin().lock(), std::io::stdout().lock(), &run) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("mike fleet receipt hook: {error}");
            0
        }
    }
}
