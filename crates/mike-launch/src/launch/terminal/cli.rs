use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

use crate::launch::error::LaunchError;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub const XDG_INSTALL_REMEDY: &str =
    "install xdg-terminal-exec and configure a terminal in xdg-terminals.list";
pub const HYPR_INSTALL_REMEDY: &str = "run mike from inside a running Hyprland session";

#[derive(Debug, Clone)]
pub struct TerminalCli {
    xdg_bin: PathBuf,
    hypr_bin: PathBuf,
}

impl TerminalCli {
    pub fn new(xdg_bin: impl Into<PathBuf>, hypr_bin: impl Into<PathBuf>) -> Self {
        Self {
            xdg_bin: xdg_bin.into(),
            hypr_bin: hypr_bin.into(),
        }
    }

    pub fn xdg_bin(&self) -> &Path {
        &self.xdg_bin
    }

    pub async fn run_xdg_text(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<String, LaunchError> {
        let output = run_raw(&self.xdg_bin, args, timeout, XDG_INSTALL_REMEDY).await?;
        if !output.status.success() {
            return Err(failure("xdg-terminal-exec", args, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn spawn_xdg(
        &self,
        cwd: &str,
        app_id: &str,
        title: &str,
        command: &[String],
        env: &BTreeMap<String, String>,
    ) -> Result<tokio::process::Child, LaunchError> {
        tokio::process::Command::new(&self.xdg_bin)
            .arg(format!("--dir={cwd}"))
            .arg(format!("--app-id={app_id}"))
            .arg(format!("--title={title}"))
            .arg("--")
            .args(command)
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    LaunchError::ExecutableNotFound {
                        name: self.xdg_bin.to_string_lossy().into_owned(),
                        remedy: XDG_INSTALL_REMEDY.to_string(),
                    }
                } else {
                    LaunchError::io(format!("spawning `{}`", self.xdg_bin.display()), e)
                }
            })
    }

    pub async fn run_hypr_json(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<Value, LaunchError> {
        let output = run_raw(&self.hypr_bin, args, timeout, HYPR_INSTALL_REMEDY).await?;
        if !output.status.success() {
            return Err(failure("hyprctl", args, &output));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| LaunchError::Malformed {
            command: command_name("hyprctl", args),
            detail: format!(
                "stdout is not JSON ({e}): {}",
                String::from_utf8_lossy(&output.stdout).trim()
            ),
        })
    }
}

async fn run_raw(
    bin: &Path,
    args: &[String],
    timeout: Duration,
    remedy: &str,
) -> Result<std::process::Output, LaunchError> {
    let command = command_name(&bin.to_string_lossy(), args);
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
                remedy: remedy.to_string(),
            })
        }
        Ok(Err(e)) => Err(LaunchError::io(format!("running `{command}`"), e)),
        Err(_) => Err(LaunchError::Timeout {
            command,
            after: timeout,
        }),
    }
}

fn command_name(prog: &str, args: &[String]) -> String {
    let head: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
    format!("{prog} {}", head.join(" ")).trim_end().to_string()
}

fn failure(prog: &str, args: &[String], output: &std::process::Output) -> LaunchError {
    let command = command_name(prog, args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    LaunchError::Failed(format!(
        "`{command}` exited with {}: {detail}",
        output.status
    ))
}

pub fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}
