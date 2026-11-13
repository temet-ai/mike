use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::launch::error::LaunchError;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub const INSTALL_REMEDY: &str = "install tmux (e.g. `pacman -S tmux`, `apt install tmux`, `brew install tmux`) and put it on PATH";

#[derive(Debug, Clone)]
pub struct TmuxCli {
    bin: PathBuf,
}

impl TmuxCli {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    pub fn bin(&self) -> &Path {
        &self.bin
    }

    pub async fn run_raw(
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

    pub async fn run_text(
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

    pub async fn run_ok(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<(), LaunchError> {
        let output = self.run_raw(args, timeout).await?;
        if !output.status.success() {
            return Err(failure(args, &output));
        }
        Ok(())
    }

    pub async fn run_missing_ok(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<bool, LaunchError> {
        let output = self.run_raw(args, timeout).await?;
        if output.status.success() {
            return Ok(true);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_not_found(&stderr) {
            return Ok(false);
        }
        Err(failure(args, &output))
    }
}

fn is_not_found(stderr: &str) -> bool {
    stderr.contains("can't find")
        || stderr.contains("session not found")
        || stderr.contains("no such")
}

pub fn command_name(args: &[String]) -> String {
    let head: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
    format!("tmux {}", head.join(" ")).trim_end().to_string()
}

fn failure(args: &[String], output: &std::process::Output) -> LaunchError {
    let command = command_name(args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    LaunchError::Failed(format!(
        "`{command}` exited with {}: {}",
        output.status,
        stderr.trim()
    ))
}

pub fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}
