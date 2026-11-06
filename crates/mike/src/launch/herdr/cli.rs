use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

use crate::launch::error::LaunchError;

pub(super) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) const INSTALL_REMEDY: &str = "install herdr from https://herdr.dev and put it on PATH";

#[derive(Debug, Clone)]
pub(super) struct HerdrCli {
    bin: PathBuf,
}

impl HerdrCli {
    pub(super) fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    pub(super) fn bin(&self) -> &Path {
        &self.bin
    }

    pub(super) async fn run_raw(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<std::process::Output, LaunchError> {
        let command = command_name(args);
        let child = tokio::process::Command::new(&self.bin)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        match tokio::time::timeout(timeout, child).await {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(LaunchError::ExecutableNotFound {
                    name: self.bin.to_string_lossy().into_owned(),
                    remedy: INSTALL_REMEDY.to_string(),
                })
            }
            Ok(Err(e)) => Err(LaunchError::io(format!("running `{command}`"), e)),
            Err(_) => Err(LaunchError::Timeout {
                command,
                after: timeout,
            }),
        }
    }

    pub(super) async fn run_text(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<String, LaunchError> {
        let output = self.run_raw(args, timeout).await?;
        if !output.status.success() {
            return Err(failure(args, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(super) async fn run_json(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<Value, LaunchError> {
        let output = self.run_raw(args, timeout).await?;
        if !output.status.success() {
            return Err(failure(args, &output));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(Value::Null);
        }
        let envelope: Value =
            serde_json::from_str(&stdout).map_err(|e| LaunchError::Malformed {
                command: command_name(args),
                detail: format!("stdout is not JSON ({e}): {}", stdout.trim()),
            })?;
        envelope
            .get("result")
            .cloned()
            .ok_or_else(|| LaunchError::Malformed {
                command: command_name(args),
                detail: format!("no `result` in {}", stdout.trim()),
            })
    }
}

pub(super) fn command_name(args: &[String]) -> String {
    let head: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
    format!("herdr {}", head.join(" ")).trim_end().to_string()
}

fn failure(args: &[String], output: &std::process::Output) -> LaunchError {
    let command = command_name(args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let structured = serde_json::from_str::<Value>(stderr.trim())
        .ok()
        .and_then(|value| value.get("error").cloned());
    if let Some(error) = structured {
        return LaunchError::BackendResponse {
            command,
            code: str_field(&error, "code").unwrap_or("error").to_string(),
            message: str_field(&error, "message").unwrap_or("").to_string(),
        };
    }
    LaunchError::Failed(format!(
        "`{command}` exited with {}: {}",
        output.status,
        stderr.trim()
    ))
}

pub(super) fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(super) fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}
