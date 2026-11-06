use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

use crate::launch::error::LaunchError;

pub(super) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) const INSTALL_REMEDY: &str =
    "install the Herdr app so `hc` and `herdr-app` are on PATH (`hc --version` answers)";

#[derive(Debug, Clone)]
pub(super) struct HcCli {
    hc: PathBuf,
    app: PathBuf,
}

impl HcCli {
    pub(super) fn new(hc: impl Into<PathBuf>, app: impl Into<PathBuf>) -> Self {
        Self {
            hc: hc.into(),
            app: app.into(),
        }
    }

    pub(super) fn hc_bin(&self) -> &std::path::Path {
        &self.hc
    }

    pub(super) async fn hc_json(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<Value, LaunchError> {
        let output = run(&self.hc, args, timeout).await?;
        parse_json(&self.hc, args, &output)
    }

    pub(super) async fn hc_text(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<String, LaunchError> {
        let output = run(&self.hc, args, timeout).await?;
        if !output.status.success() {
            return Err(failure(&self.hc, args, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(super) async fn app_json(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<Value, LaunchError> {
        let output = run(&self.app, args, timeout).await?;
        parse_json(&self.app, args, &output)
    }
}

async fn run(
    bin: &std::path::Path,
    args: &[String],
    timeout: Duration,
) -> Result<std::process::Output, LaunchError> {
    let child = tokio::process::Command::new(bin)
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
                name: bin.to_string_lossy().into_owned(),
                remedy: INSTALL_REMEDY.to_string(),
            })
        }
        Ok(Err(e)) => Err(LaunchError::io(
            format!("running `{}`", command_name(bin, args)),
            e,
        )),
        Err(_) => Err(LaunchError::Timeout {
            command: command_name(bin, args),
            after: timeout,
        }),
    }
}

fn parse_json(
    bin: &std::path::Path,
    args: &[String],
    output: &std::process::Output,
) -> Result<Value, LaunchError> {
    if !output.status.success() {
        return Err(failure(bin, args, output));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.lines().rev().find(|l| !l.trim().is_empty());
    let Some(line) = last else {
        return Ok(Value::Null);
    };
    serde_json::from_str(stdout.trim())
        .or_else(|_| serde_json::from_str(line.trim()))
        .map_err(|e| LaunchError::Malformed {
            command: command_name(bin, args),
            detail: format!("stdout is not JSON ({e}): {}", stdout.trim()),
        })
}

pub(super) fn command_name(bin: &std::path::Path, args: &[String]) -> String {
    let name = bin
        .file_name()
        .map_or_else(|| bin.to_string_lossy(), |n| n.to_string_lossy());
    let head: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
    format!("{name} {}", head.join(" ")).trim_end().to_string()
}

fn failure(bin: &std::path::Path, args: &[String], output: &std::process::Output) -> LaunchError {
    let command = command_name(bin, args);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let structured = [stdout.trim(), stderr.trim()].into_iter().find_map(|text| {
        serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.get("error").cloned())
    });
    if let Some(error) = structured {
        return LaunchError::BackendResponse {
            command,
            code: str_field(&error, "kind").unwrap_or("error").to_string(),
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
