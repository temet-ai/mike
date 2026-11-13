use std::io::{Read, Write};
use std::path::Path;

use serde_json::{Value, json};

#[path = "background_hook_syntax.rs"]
pub mod syntax;
#[cfg(test)]
#[path = "background_hook_tests.rs"]
mod tests;

pub const COMMANDS_ENV: &str = "MIKE_FLEET_BACKGROUND_COMMANDS";
const BACKGROUND_DISABLED: &str = "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS";
const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;

fn declared(text: Option<&str>) -> Vec<String> {
    text.and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default()
}

pub fn append_hook(settings: &mut Value, executable: &Path) {
    if settings["disableAllHooks"] == true
        || declared(settings["env"][COMMANDS_ENV].as_str()).is_empty()
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
            "command": format!("'{escaped}' fleet background-hook"),
            "timeout": 5,
        }],
    });
    if !before.contains(&handler) {
        before.push(handler);
    }
}

fn response(event: &Value, commands: &[String]) -> Option<Value> {
    if event["hook_event_name"] != "PreToolUse" || event["tool_name"] != "Bash" {
        return None;
    }
    let mut input = event.get("tool_input")?.as_object()?.clone();
    if input.get("run_in_background") == Some(&json!(true)) {
        return None;
    }
    let command = input.get("command")?.as_str()?;
    if !syntax::declared_command(command, commands) {
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

fn process(input: impl Read, mut output: impl Write, commands: &[String]) -> Result<(), String> {
    let mut bytes = Vec::new();
    input
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("hook input exceeded 4 MiB; no tool input was changed".into());
    }
    let event: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let response = response(&event, commands).unwrap_or_else(|| json!({}));
    serde_json::to_writer(&mut output, &response).map_err(|error| error.to_string())
}

pub fn run() -> i32 {
    let run = std::env::var("MIKE_RUN_ID").ok();
    let commands = declared(std::env::var(COMMANDS_ENV).ok().as_deref());
    let disabled = std::env::var(BACKGROUND_DISABLED).ok();
    if !fleet_enabled(run.as_deref(), &commands, disabled.as_deref()) {
        return 0;
    }
    match process(std::io::stdin().lock(), std::io::stdout().lock(), &commands) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("mike fleet background hook: {error}");
            1
        }
    }
}

fn fleet_enabled(run: Option<&str>, commands: &[String], disabled: Option<&str>) -> bool {
    run.is_some_and(|run| run.starts_with("fleet-") && run.len() > "fleet-".len())
        && !commands.is_empty()
        && disabled != Some("1")
}
