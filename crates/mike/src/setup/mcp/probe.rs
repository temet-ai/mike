use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::process::{Child, Command};

use super::ProbeReport;

const STEP_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_FRAME_BYTES: u64 = 8 * 1024 * 1024;

pub(super) async fn run(
    binary: &Path,
    inspect_url: &str,
    instance_id: &str,
) -> Result<ProbeReport> {
    verify_version(binary).await?;
    let mut child = command(binary, inspect_url)
        .arg("--core")
        .spawn()
        .context("could not launch the MCP companion")?;
    let outcome = exchange(&mut child, instance_id).await;
    stop(&mut child).await?;
    let (handshake, backend_read, code, detail) = outcome;
    Ok(ProbeReport {
        binary: binary.to_path_buf(),
        inspect_url: inspect_url.to_string(),
        instance_id: instance_id.to_string(),
        version: env!("CARGO_PKG_VERSION"),
        handshake,
        backend_read,
        code,
        detail,
    })
}

fn command(binary: &Path, inspect_url: &str) -> Command {
    let mut command = Command::new(binary);
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env_remove(key);
    }
    command.env("NO_PROXY", "*").env("no_proxy", "*");
    command
        .env("MIKE_INSPECT_URL", inspect_url)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("MIKE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}

async fn verify_version(binary: &Path) -> Result<()> {
    let mut child = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("could not execute MCP --version")?;
    let outcome = tokio::time::timeout(STEP_TIMEOUT, async {
        let stdout = child
            .stdout
            .take()
            .context("MCP version stdout is unavailable")?;
        let mut bytes = Vec::new();
        stdout.take(4097).read_to_end(&mut bytes).await?;
        let status = child.wait().await?;
        anyhow::ensure!(status.success(), "MCP --version exited unsuccessfully");
        check_version_output(&bytes)
    })
    .await;
    stop(&mut child).await?;
    outcome.context("MCP --version timed out; companion was stopped")?
}

pub(super) fn check_version_output(bytes: &[u8]) -> Result<()> {
    anyhow::ensure!(
        bytes.len() <= 4096,
        "MCP --version returned an invalid response"
    );
    let expected = format!("mike-mcp {}", env!("CARGO_PKG_VERSION"));
    anyhow::ensure!(
        std::str::from_utf8(bytes)?.trim() == expected,
        "MCP companion version does not match mike {}; install the matching pair",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

async fn stop(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        child
            .start_kill()
            .context("could not stop MCP probe child")?;
    }
    tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .context("MCP probe child did not stop within two seconds")??;
    Ok(())
}

async fn exchange(
    child: &mut Child,
    instance_id: &str,
) -> (bool, bool, &'static str, &'static str) {
    let Some(stdin) = child.stdin.take() else {
        return handshake_failed();
    };
    let Some(stdout) = child.stdout.take() else {
        return handshake_failed();
    };
    converse(stdin, BufReader::new(stdout), instance_id, STEP_TIMEOUT).await
}

pub(super) async fn converse<W, R>(
    stdin: W,
    stdout: R,
    instance_id: &str,
    step: Duration,
) -> (bool, bool, &'static str, &'static str)
where
    W: AsyncWrite + Unpin,
    R: AsyncBufRead + Unpin,
{
    let mut wire = Wire { stdin, stdout };
    if !matches!(
        tokio::time::timeout(step, wire.initialize()).await,
        Ok(Ok(()))
    ) {
        return handshake_failed();
    }
    match tokio::time::timeout(step, wire.read_backend(instance_id)).await {
        Ok(Ok(BackendRead::Verified)) => (
            true,
            true,
            "MCP_READY",
            "Matching companion passed stdio initialize, tools/list and a read-only get_stats call against the selected inspect endpoint.",
        ),
        Ok(Ok(BackendRead::Unavailable)) | Err(_) => (
            true,
            false,
            "MCP_BACKEND_UNAVAILABLE",
            "Stdio initialize and tools/list passed; the selected inspect backend could not complete the read. Start that instance and run mike mcp probe again.",
        ),
        Ok(Ok(BackendRead::WrongInstance)) => (
            true,
            false,
            "MCP_INSTANCE_MISMATCH",
            "MCP read reached an instance that does not match the selected config, or lacks boot identity. No registration was written; stop the conflicting instance or select its actual config.",
        ),
        Ok(Err(_)) => (
            true,
            false,
            "MCP_READ_PROTOCOL_FAILED",
            "Stdio initialize and tools/list passed, but the read response was not valid MCP. Check the companion and run mike mcp probe again.",
        ),
    }
}

fn handshake_failed() -> (bool, bool, &'static str, &'static str) {
    (
        false,
        false,
        "MCP_HANDSHAKE_FAILED",
        "Companion version matched, but stdio initialize/tools-list failed or timed out. No client config was changed; check the matching mike-mcp installation.",
    )
}

enum BackendRead {
    Verified,
    Unavailable,
    WrongInstance,
}

struct Wire<W, R> {
    stdin: W,
    stdout: R,
}

impl<W: AsyncWrite + Unpin, R: AsyncBufRead + Unpin> Wire<W, R> {
    async fn initialize(&mut self) -> Result<()> {
        self.send(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2024-11-05","capabilities":{},
                "clientInfo":{"name":"mike-connection-check","version":env!("CARGO_PKG_VERSION")}
            }}),
        )
        .await?;
        let response = self.receive(1).await?;
        let result = response
            .get("result")
            .context("MCP initialize was rejected")?;
        anyhow::ensure!(
            result["serverInfo"]["name"] == "mike-mcp"
                && result["serverInfo"]["version"] == env!("CARGO_PKG_VERSION")
                && result["capabilities"]["tools"].is_object()
                && result["protocolVersion"] == "2024-11-05",
            "MCP initialize identity/protocol is invalid"
        );
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await?;
        self.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))
            .await?;
        let response = self.receive(2).await?;
        let tools = response["result"]["tools"]
            .as_array()
            .context("MCP tools/list was rejected")?;
        anyhow::ensure!(
            tools.iter().any(|tool| tool["name"] == "get_stats"),
            "MCP core profile does not advertise get_stats"
        );
        Ok(())
    }

    async fn read_backend(&mut self, instance_id: &str) -> Result<BackendRead> {
        self.send(
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
                "name":"get_stats","arguments":{}
            }}),
        )
        .await?;
        let response = self.receive(3).await?;
        if response.get("error").is_some() {
            return Ok(BackendRead::Unavailable);
        }
        let result = response
            .get("result")
            .context("MCP read response has no result")?;
        if result["isError"] == true {
            return Ok(BackendRead::Unavailable);
        }
        let content = result["content"]
            .as_array()
            .context("MCP read response has no content")?;
        let stats = content
            .iter()
            .filter(|item| item["type"] == "text")
            .filter_map(|item| item["text"].as_str())
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .find(|value| value["stats"].is_object())
            .context("MCP read did not return stats JSON text content")?;
        if matches_instance(&stats, instance_id) {
            Ok(BackendRead::Verified)
        } else {
            Ok(BackendRead::WrongInstance)
        }
    }

    async fn send(&mut self, value: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn receive(&mut self, id: u64) -> Result<Value> {
        loop {
            let mut bytes = Vec::new();
            let read = (&mut self.stdout)
                .take(MAX_FRAME_BYTES + 1)
                .read_until(b'\n', &mut bytes)
                .await?;
            anyhow::ensure!(read > 0, "MCP companion closed stdout");
            anyhow::ensure!(
                bytes.len() as u64 <= MAX_FRAME_BYTES,
                "MCP protocol frame is too large"
            );
            let response: Value = serde_json::from_slice(&bytes)
                .context("MCP stdout contains non-protocol output")?;
            anyhow::ensure!(
                response["jsonrpc"] == "2.0",
                "MCP protocol version marker is invalid"
            );
            if response.get("id").is_none() {
                anyhow::ensure!(response["method"].is_string(), "invalid MCP notification");
                continue;
            }
            anyhow::ensure!(response["id"] == id, "unexpected MCP response id");
            return Ok(response);
        }
    }
}

fn matches_instance(stats: &Value, instance_id: &str) -> bool {
    let instance = &stats["stats"]["health"]["instance"];
    instance["instance_id"].as_str() == Some(instance_id)
        && instance["boot_generation"]
            .as_str()
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok())
}

#[cfg(test)]
mod proxy_tests {
    use super::*;

    #[test]
    fn probe_child_removes_ambient_proxy_routes_and_bypasses_all_proxies() {
        let child = command(Path::new("/fixture/mike-mcp"), "http://127.0.0.1:9989");
        let env: std::collections::BTreeMap<_, _> = child.as_std().get_envs().collect();
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None));
        }
        for key in ["NO_PROXY", "no_proxy"] {
            assert_eq!(
                env.get(std::ffi::OsStr::new(key)),
                Some(&Some(std::ffi::OsStr::new("*")))
            );
        }
    }
}
