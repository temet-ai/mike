use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdout, Command};
use tokio::time::timeout;

pub(crate) const BINARY: &str = env!("CARGO_BIN_EXE_mike-mcp");

pub(crate) fn initialize_request() -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "integration-test",
                "version": "1.0.0"
            }
        }
    })
    .to_string()
}

pub(crate) fn initialized_notification() -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
    .to_string()
}

pub(crate) fn list_tools_request(id: u32) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list"
    })
    .to_string()
}

pub(crate) fn call_list_requests_tool(id: u32) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": "list_requests",
            "arguments": {}
        }
    })
    .to_string()
}

pub(crate) async fn read_next_message(
    reader: &mut BufReader<ChildStdout>,
) -> anyhow::Result<Value> {
    let mut line = String::new();
    let read_result = timeout(Duration::from_secs(5), reader.read_line(&mut line)).await;
    match read_result {
        Ok(Ok(0)) => anyhow::bail!("stdout closed before reading a message"),
        Ok(Ok(_)) => {
            let value: Value = serde_json::from_str(line.trim())
                .map_err(|e| anyhow::anyhow!("bad JSON from mike-mcp stdout: {e}: {line}"))?;
            Ok(value)
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("stdout read error: {e}")),
        Err(_) => Err(anyhow::anyhow!("timed out waiting for stdout message")),
    }
}

pub(crate) fn call_tool_request(id: u32, name: &str, arguments: Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments,
        }
    })
    .to_string()
}

pub(crate) async fn spawn_mcp_with_inspect(
    inspect_url: &str,
) -> anyhow::Result<(
    tokio::process::Child,
    tokio::process::ChildStdin,
    BufReader<ChildStdout>,
)> {
    let mut child = Command::new(BINARY)
        .env("MIKE_INSPECT_URL", inspect_url)
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdout"))?;
    let mut reader = BufReader::new(stdout);

    stdin
        .write_all(format!("{}\n", initialize_request()).as_bytes())
        .await?;
    stdin
        .write_all(format!("{}\n", initialized_notification()).as_bytes())
        .await?;
    let _init = read_next_message(&mut reader).await?;

    Ok((child, stdin, reader))
}

pub(crate) fn structured_content(response: &Value) -> anyhow::Result<&Value> {
    let result = &response["result"];
    if result["isError"] == Value::Bool(true) {
        anyhow::bail!("tool returned isError=true: {response}");
    }
    let structured = result
        .get("structuredContent")
        .ok_or_else(|| anyhow::anyhow!("no structuredContent in response: {response}"))?;
    Ok(structured)
}
