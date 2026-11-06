use std::io::{Read, Write};
use std::path::Path;

use serde_json::{Value, json};

#[path = "cargo_background_syntax.rs"]
mod syntax;
#[cfg(test)]
#[path = "cargo_background_tests.rs"]
mod tests;

pub(super) const ENABLED_ENV: &str = "MIKE_FLEET_CARGO_BACKGROUND";
const BACKGROUND_DISABLED: &str = "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS";
const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;

pub(in crate::launch::fleet) fn append_hook(settings: &mut Value, executable: &Path) {
    if settings["disableAllHooks"] == true
        || settings["env"][ENABLED_ENV] != "1"
        || settings["env"][BACKGROUND_DISABLED] == "1"
    {
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
            "command": format!("'{escaped}' fleet cargo-background-hook"),
            "timeout": 5,
        }],
    });
    if !before.contains(&handler) {
        before.push(handler);
    }
}

fn response(event: &Value) -> Option<Value> {
    if event["hook_event_name"] != "PreToolUse" || event["tool_name"] != "Bash" {
        return None;
    }
    let mut input = event.get("tool_input")?.as_object()?.clone();
    if input.get("run_in_background") == Some(&json!(true)) {
        return None;
    }
    let command = input.get("command")?.as_str()?;
    if !syntax::cargo_build(command) {
        return None;
    }
    input.insert("run_in_background".into(), json!(true));
    Some(json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "updatedInput": input,
            "additionalContext": "Fleet safeguard requested native background execution. A launch task ID is pending work, not a passed check. Read the native task output on completion. For Mike's durable queue and completion mail, use fleet_check(command).",
        },
    }))
}

fn process(input: impl Read, mut output: impl Write) -> Result<(), String> {
    let mut bytes = Vec::new();
    input
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("hook input exceeded 4 MiB; no tool input was changed".into());
    }
    let event: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let response = response(&event).unwrap_or_else(|| json!({}));
    serde_json::to_writer(&mut output, &response).map_err(|error| error.to_string())
}

pub fn run() -> i32 {
    let run = std::env::var("MIKE_RUN_ID").ok();
    let enabled = std::env::var(ENABLED_ENV).ok();
    let disabled = std::env::var(BACKGROUND_DISABLED).ok();
    if !fleet_enabled(run.as_deref(), enabled.as_deref(), disabled.as_deref()) {
        return 0;
    }
    match process(std::io::stdin().lock(), std::io::stdout().lock()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("mike fleet cargo background hook: {error}");
            1
        }
    }
}

fn fleet_enabled(run: Option<&str>, enabled: Option<&str>, disabled: Option<&str>) -> bool {
    run.is_some_and(|run| run.starts_with("fleet-") && run.len() > "fleet-".len())
        && enabled == Some("1")
        && disabled != Some("1")
}
