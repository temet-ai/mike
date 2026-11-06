use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use super::env::clean_cli_env;
use super::error::AnthropicCliError;

pub(crate) const DEFAULT_STATUS_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone)]
pub(crate) struct AnthropicCli {
    bin: PathBuf,
}

impl AnthropicCli {
    pub(crate) fn with_binary(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    pub(crate) fn bin(&self) -> &Path {
        &self.bin
    }

    pub(crate) fn binary_label(&self) -> String {
        self.bin.to_string_lossy().into_owned()
    }

    pub(crate) async fn run_text(
        &self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, AnthropicCliError> {
        let output = self.run_raw(args, timeout).await?;
        if !output.status.success() {
            return Err(failed(args, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn run_raw(
        &self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<std::process::Output, AnthropicCliError> {
        let env = clean_cli_env();
        let mut command = tokio::process::Command::new(&self.bin);
        command.args(args);
        command.env_clear();
        command.envs(&env);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);
        match tokio::time::timeout(timeout, command.output()).await {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(AnthropicCliError::NotFound)
            }
            Ok(Err(e)) => Err(AnthropicCliError::Io(e.to_string())),
            Err(_) => Err(AnthropicCliError::Timeout),
        }
    }
}

fn failed(args: &[&str], output: &std::process::Output) -> AnthropicCliError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    AnthropicCliError::Failed(format!(
        "`claude {}` exited with {}: {}",
        args.join(" "),
        output.status,
        stderr.trim()
    ))
}
