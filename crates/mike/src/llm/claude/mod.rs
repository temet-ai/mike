use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::process::Command;

use super::LlmError;
use crate::environment::{Environment, ProcessEnvironment};

#[cfg(test)]
mod tests;

pub(crate) const MODEL_PREFIX: &str = "claude/";
pub(crate) const DEFAULT_BIN: &str = "claude";
pub(crate) const BIN_ENV: &str = "MIKE_CLAUDE_CLI_BIN";
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const SYSTEM_PROMPT_FILE: &str = "system-prompt.txt";

pub(crate) struct ClaudeRequest<'a> {
    pub(crate) model: &'a str,
    pub(crate) system: &'a str,
    pub(crate) user: &'a str,
    pub(crate) schema: &'a Value,
    pub(crate) reasoning_effort: &'a str,
    pub(crate) timeout: Duration,
}

pub(crate) fn binary() -> String {
    std::env::var(BIN_ENV)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_BIN.to_owned())
}

pub(crate) fn claude_effort(effort: &str) -> Option<&str> {
    match effort {
        "low" | "medium" | "high" | "xhigh" | "max" => Some(effort),
        _ => None,
    }
}

pub(crate) fn cli_args(
    model: &str,
    effort: Option<&str>,
    schema_json: &str,
    system_file: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--no-session-persistence",
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--strict-mcp-config",
        "--tools",
        "",
        "--model",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.push(model.to_owned());
    if let Some(effort) = effort {
        args.push("--effort".to_owned());
        args.push(effort.to_owned());
    }
    args.push("--json-schema".to_owned());
    args.push(schema_json.to_owned());
    args.push("--system-prompt-file".to_owned());
    args.push(system_file.display().to_string());
    args
}

pub(crate) fn result_object(stdout: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(stdout.trim()).ok()?;
    match parsed {
        Value::Array(items) => items
            .into_iter()
            .rev()
            .find(|item| item.get("type").and_then(Value::as_str) == Some("result")),
        object @ Value::Object(_) => Some(object),
        _ => None,
    }
}

fn error_text(result: &Value) -> String {
    let text = result
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("claude returned an error without a message");
    match result.get("api_error_status").and_then(Value::as_i64) {
        Some(status) => format!("{text} (API status {status})"),
        None => text.to_owned(),
    }
}

pub(crate) fn failure_message(stdout: &str, stderr: &str) -> String {
    let from_result = result_object(stdout)
        .filter(|r| r.get("is_error").and_then(Value::as_bool) == Some(true))
        .map(|r| error_text(&r));
    from_result.unwrap_or_else(|| {
        let stderr = stderr.trim();
        if stderr.is_empty() {
            stdout.trim().to_owned()
        } else {
            stderr.to_owned()
        }
    })
}

pub(crate) fn effort_rejected(message: &str) -> bool {
    message.to_ascii_lowercase().contains("effort")
}

fn token_count(usage: &Value, field: &str) -> i64 {
    usage.get(field).and_then(Value::as_i64).unwrap_or(0)
}

fn resolved_model(result: &Value, requested: &str) -> String {
    result
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|models| models.keys().next().cloned())
        .unwrap_or_else(|| requested.to_owned())
}

pub(crate) fn completion_payload(model: &str, result: &Value) -> Result<Value, String> {
    if result.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(error_text(result));
    }
    let subtype = result.get("subtype").and_then(Value::as_str);
    if subtype.is_some_and(|s| s != "success") {
        return Err(format!(
            "claude ended without a result: {}",
            subtype.unwrap_or_default()
        ));
    }
    let content = match result.get("structured_output") {
        Some(structured) if !structured.is_null() => structured.to_string(),
        _ => result
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "claude returned neither a structured output nor a result".to_owned())?,
    };
    let usage = result.get("usage").cloned().unwrap_or(Value::Null);
    let cache_read = token_count(&usage, "cache_read_input_tokens");
    let prompt_tokens = token_count(&usage, "input_tokens")
        + token_count(&usage, "cache_creation_input_tokens")
        + cache_read;
    Ok(json!({
        "id": result.get("session_id"),
        "model": resolved_model(result, model),
        "choices": [{
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": token_count(&usage, "output_tokens"),
            "completion_tokens_details": {
                "reasoning_tokens": usage
                    .pointer("/output_tokens_details/thinking_tokens")
                    .and_then(Value::as_i64)
            },
            "prompt_tokens_details": {"cached_tokens": cache_read},
            "cost": null
        }
    }))
}

pub(crate) fn resolved_path(bin: &str) -> String {
    let direct = Path::new(bin);
    if direct.components().count() > 1 {
        return direct.display().to_string();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join(bin))
        .find(|candidate| candidate.is_file())
        .map_or_else(
            || format!("{bin} (not found on PATH)"),
            |found| found.display().to_string(),
        )
}

fn transport(message: String) -> LlmError {
    LlmError::Transport(message)
}

struct ExecFailure {
    message: String,
}

async fn run_once(
    bin: &str,
    req: &ClaudeRequest<'_>,
    effort: Option<&str>,
    environment: &dyn Environment,
) -> Result<Result<Value, ExecFailure>, LlmError> {
    let workdir = tempfile::Builder::new()
        .prefix("mike-claude-")
        .tempdir()
        .map_err(|e| transport(format!("claude workdir: {e}")))?;
    let system_file = workdir.path().join(SYSTEM_PROMPT_FILE);
    tokio::fs::write(&system_file, req.system.as_bytes())
        .await
        .map_err(|e| transport(format!("claude system prompt file: {e}")))?;
    let schema_json = serde_json::to_string(req.schema)?;

    let mut child = Command::new(bin)
        .args(cli_args(req.model, effort, &schema_json, &system_file))
        .env_clear()
        .envs(crate::anthropic_cli::clean_cli_env_from(environment))
        .current_dir(workdir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            transport(format!(
                "claude could not start (resolved path {}): {e}",
                resolved_path(bin)
            ))
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        super::child_stdin::write_prompt(&mut stdin, req.user.as_bytes())
            .await
            .map_err(|e| transport(format!("claude stdin: {e}")))?;
    }

    let output = tokio::time::timeout(req.timeout, child.wait_with_output())
        .await
        .map_err(|_| {
            tracing::warn!(
                path = %resolved_path(bin),
                timeout_ms = req.timeout.as_millis(),
                "claude produced no result before the timeout"
            );
            LlmError::Timeout
        })?
        .map_err(|e| transport(format!("claude: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stdout.trim().is_empty() && stderr.trim().is_empty() {
        return Ok(Err(ExecFailure {
            message: format!(
                "claude at {} exited {} and produced no output",
                resolved_path(bin),
                output.status
            ),
        }));
    }
    if !output.status.success() {
        return Ok(Err(ExecFailure {
            message: format!(
                "claude exited {}: {}",
                output.status,
                failure_message(&stdout, &stderr)
            ),
        }));
    }
    let outcome = result_object(&stdout)
        .ok_or_else(|| "claude printed no JSON result".to_owned())
        .and_then(|result| completion_payload(req.model, &result));
    Ok(outcome.map_err(|message| ExecFailure {
        message: format!("claude: {message}"),
    }))
}

pub(crate) async fn run(bin: &str, req: ClaudeRequest<'_>) -> Result<Value, LlmError> {
    run_in(bin, req, &ProcessEnvironment).await
}

pub(crate) async fn run_in(
    bin: &str,
    req: ClaudeRequest<'_>,
    environment: &dyn Environment,
) -> Result<Value, LlmError> {
    let effort = claude_effort(req.reasoning_effort);
    let failure = match run_once(bin, &req, effort, environment).await? {
        Ok(payload) => return Ok(payload),
        Err(failure) => failure,
    };
    if effort.is_none() || !effort_rejected(&failure.message) {
        return Err(transport(failure.message));
    }
    tracing::warn!(
        model = req.model,
        error = %failure.message,
        "claude: model rejected the reasoning effort; resending without it"
    );
    match run_once(bin, &req, None, environment).await? {
        Ok(payload) => Ok(payload),
        Err(retry) => Err(transport(retry.message)),
    }
}
