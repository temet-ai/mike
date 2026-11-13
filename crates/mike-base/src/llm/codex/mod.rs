use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::process::Command;

use super::LlmError;

pub mod login;
#[cfg(test)]
pub mod tests;

pub use login::{CodexLogin, login_status};

pub const MODEL_PREFIX: &str = "codex/";
pub const DEFAULT_BIN: &str = "codex";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const SCHEMA_FILE: &str = "schema.json";
const LAST_MESSAGE_FILE: &str = "last-message.txt";
const RETRY_EFFORT: &str = "low";

pub struct CodexRequest<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub schema: &'a Value,
    pub reasoning_effort: &'a str,
    pub timeout: Duration,
}

pub fn codex_reasoning_effort(effort: &str) -> &str {
    match effort {
        "" => "none",
        other => other,
    }
}

pub fn compose_prompt(system: &str, user: &str) -> String {
    format!("<instructions>\n{system}\n</instructions>\n\n{user}")
}

pub fn exec_args(model: &str, reasoning_effort: &str, workdir: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "exec",
        "--ephemeral",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
        "-s",
        "read-only",
        "-C",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.push(workdir.display().to_string());
    args.push("-m".to_owned());
    args.push(model.to_owned());
    args.push("-c".to_owned());
    args.push(format!(
        "model_reasoning_effort={}",
        codex_reasoning_effort(reasoning_effort)
    ));
    args.push("--output-schema".to_owned());
    args.push(workdir.join(SCHEMA_FILE).display().to_string());
    args.push("-o".to_owned());
    args.push(workdir.join(LAST_MESSAGE_FILE).display().to_string());
    args.push("--json".to_owned());
    args.push("-".to_owned());
    args
}

fn events(stdout: &str) -> impl Iterator<Item = Value> + '_ {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

pub fn failure_message(stdout: &str, stderr: &str) -> String {
    let from_events = events(stdout)
        .filter(|e| {
            matches!(
                e.get("type").and_then(Value::as_str),
                Some("error" | "turn.failed")
            )
        })
        .filter_map(|e| {
            e.get("message")
                .or_else(|| e.pointer("/error/message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .last();
    from_events.unwrap_or_else(|| stderr.trim().to_owned())
}

pub fn reasoning_effort_rejected(message: &str) -> bool {
    message.contains("reasoning.effort") || message.contains("reasoning effort")
}

pub fn completion_payload(model: &str, stdout: &str, last_message: &str) -> Value {
    let mut thread_id = None;
    let mut usage = Value::Null;
    for event in events(stdout) {
        match event.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                thread_id = event
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("turn.completed") => {
                usage = event.get("usage").cloned().unwrap_or(Value::Null);
            }
            _ => {}
        }
    }
    json!({
        "id": thread_id,
        "model": model,
        "choices": [{
            "message": {"role": "assistant", "content": last_message},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": usage.get("input_tokens"),
            "completion_tokens": usage.get("output_tokens"),
            "completion_tokens_details": {
                "reasoning_tokens": usage.get("reasoning_output_tokens")
            },
            "prompt_tokens_details": {
                "cached_tokens": usage.get("cached_input_tokens")
            },
            "cost": null
        }
    })
}

fn transport(message: String) -> LlmError {
    LlmError::Transport(message)
}

struct ExecFailure {
    message: String,
}

async fn run_once(
    bin: &str,
    req: &CodexRequest<'_>,
    reasoning_effort: &str,
) -> Result<Result<Value, ExecFailure>, LlmError> {
    let workdir = tempfile::Builder::new()
        .prefix("mike-codex-")
        .tempdir()
        .map_err(|e| transport(format!("codex workdir: {e}")))?;
    tokio::fs::write(
        workdir.path().join(SCHEMA_FILE),
        serde_json::to_vec(req.schema)?,
    )
    .await
    .map_err(|e| transport(format!("codex schema file: {e}")))?;

    let mut child = Command::new(bin)
        .args(exec_args(req.model, reasoning_effort, workdir.path()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| transport(format!("codex exec could not start ({bin}): {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        super::child_stdin::write_prompt(
            &mut stdin,
            compose_prompt(req.system, req.user).as_bytes(),
        )
        .await
        .map_err(|e| transport(format!("codex exec stdin: {e}")))?;
    }

    let output = tokio::time::timeout(req.timeout, child.wait_with_output())
        .await
        .map_err(|_| LlmError::Timeout)?
        .map_err(|e| transport(format!("codex exec: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(Err(ExecFailure {
            message: format!(
                "codex exec exited {}: {}",
                output.status,
                failure_message(&stdout, &stderr)
            ),
        }));
    }

    let last_message = tokio::fs::read_to_string(workdir.path().join(LAST_MESSAGE_FILE))
        .await
        .map_err(|_| transport("codex exec produced no final message".to_owned()))?;
    Ok(Ok(completion_payload(
        req.model,
        &stdout,
        last_message.trim(),
    )))
}

pub async fn run(bin: &str, req: CodexRequest<'_>) -> Result<Value, LlmError> {
    let effort = codex_reasoning_effort(req.reasoning_effort);
    let failure = match run_once(bin, &req, effort).await? {
        Ok(payload) => return Ok(payload),
        Err(failure) => failure,
    };
    if effort == RETRY_EFFORT || !reasoning_effort_rejected(&failure.message) {
        return Err(transport(failure.message));
    }
    tracing::warn!(
        model = req.model,
        effort,
        error = %failure.message,
        "codex: model rejected the reasoning effort; resending at {RETRY_EFFORT}"
    );
    match run_once(bin, &req, RETRY_EFFORT).await? {
        Ok(payload) => Ok(payload),
        Err(retry) => Err(transport(retry.message)),
    }
}
