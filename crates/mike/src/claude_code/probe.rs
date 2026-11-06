use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::error::ClaudeCodeError;

pub(crate) const PROBE_WAIT: Duration = Duration::from_secs(10);
const PROTOCOL_VERSION: &str = "2025-11-25";

fn request(id: u32, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

fn initialize_message() -> String {
    request(
        1,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "mike-setup", "version": env!("CARGO_PKG_VERSION") },
        }),
    )
}

fn initialized_message() -> String {
    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string()
}

fn list_tools_message() -> String {
    json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }).to_string()
}

async fn send(stdin: &mut ChildStdin, line: &str) -> Result<(), ClaudeCodeError> {
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|e| ClaudeCodeError::Probe(format!("could not write to mike-mcp: {e}")))
}

async fn answer_to(
    lines: &mut Lines<BufReader<ChildStdout>>,
    id: u64,
) -> Result<Value, ClaudeCodeError> {
    loop {
        let line = lines
            .next_line()
            .await
            .map_err(|e| ClaudeCodeError::Probe(format!("could not read from mike-mcp: {e}")))?
            .ok_or_else(|| {
                ClaudeCodeError::Probe(
                    "it exited before it answered. Run it by hand to see why".to_string(),
                )
            })?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("id").and_then(Value::as_u64) == Some(id) {
            return match message.get("error") {
                Some(error) => Err(ClaudeCodeError::Probe(format!(
                    "it answered request {id} with an error: {error}"
                ))),
                None => Ok(message),
            };
        }
    }
}

async fn exchange(child: &mut Child) -> Result<usize, ClaudeCodeError> {
    let missing = |what: &str| ClaudeCodeError::Probe(format!("no {what} pipe to mike-mcp"));
    let mut stdin = child.stdin.take().ok_or_else(|| missing("input"))?;
    let stdout = child.stdout.take().ok_or_else(|| missing("output"))?;
    let mut lines = BufReader::new(stdout).lines();
    send(&mut stdin, &initialize_message()).await?;
    send(&mut stdin, &initialized_message()).await?;
    answer_to(&mut lines, 1).await?;
    send(&mut stdin, &list_tools_message()).await?;
    let listed = answer_to(&mut lines, 2).await?;
    listed
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| ClaudeCodeError::Probe("tools/list returned no tools list".to_string()))
}

pub(crate) async fn tool_count(
    binary: &Path,
    inspect_url: &str,
    wait: Duration,
) -> Result<usize, ClaudeCodeError> {
    super::refuse_in_sandbox("starting mike-mcp")?;
    tool_count_unguarded(binary, inspect_url, wait).await
}

pub(crate) async fn tool_count_unguarded(
    binary: &Path,
    inspect_url: &str,
    wait: Duration,
) -> Result<usize, ClaudeCodeError> {
    let mut child = Command::new(binary)
        .env("MIKE_INSPECT_URL", inspect_url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            ClaudeCodeError::Probe(format!("could not start {}: {e}", binary.display()))
        })?;
    match tokio::time::timeout(wait, exchange(&mut child)).await {
        Ok(result) => result,
        Err(_) => Err(ClaudeCodeError::Probe(format!(
            "no answer within {} seconds",
            wait.as_secs()
        ))),
    }
}
