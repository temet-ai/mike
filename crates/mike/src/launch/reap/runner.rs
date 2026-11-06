use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;

const RUN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcRunOutput {
    pub(crate) stdout: String,
    pub(crate) exit_code: i32,
    pub(crate) exec_error: Option<String>,
}

impl ProcRunOutput {
    pub(crate) fn ok(&self) -> bool {
        self.exec_error.is_none() && self.exit_code == 0
    }
}

#[async_trait]
pub(crate) trait ProcRunner: Send + Sync {
    async fn run(&self, name: &str, args: &[String]) -> ProcRunOutput;
}

pub(crate) struct SystemRunner;

#[async_trait]
impl ProcRunner for SystemRunner {
    async fn run(&self, name: &str, args: &[String]) -> ProcRunOutput {
        let mut command = tokio::process::Command::new(name);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if name == "tmux" {
            command.env_remove("TMUX").env_remove("TMUX_PANE");
        }
        let child = command.output();
        match tokio::time::timeout(RUN_TIMEOUT, child).await {
            Ok(Ok(output)) => ProcRunOutput {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                exit_code: output.status.code().unwrap_or(-1),
                exec_error: None,
            },
            Ok(Err(e)) => ProcRunOutput {
                stdout: String::new(),
                exit_code: -1,
                exec_error: Some(e.to_string()),
            },
            Err(_) => ProcRunOutput {
                stdout: String::new(),
                exit_code: -1,
                exec_error: Some(format!("`{name}` did not answer within {RUN_TIMEOUT:?}")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_is_only_true_for_a_clean_exit() {
        let clean = ProcRunOutput {
            stdout: String::new(),
            exit_code: 0,
            exec_error: None,
        };
        assert!(clean.ok());
        let nonzero = ProcRunOutput {
            exit_code: 1,
            ..clean.clone()
        };
        assert!(!nonzero.ok());
        let failed_exec = ProcRunOutput {
            exec_error: Some("not found".into()),
            ..clean
        };
        assert!(!failed_exec.ok());
    }

    #[tokio::test]
    async fn system_runner_missing_executable_is_an_exec_error_not_a_panic() {
        let output = SystemRunner.run("/nonexistent/binary-xyz", &[]).await;
        assert!(output.exec_error.is_some());
        assert!(!output.ok());
    }

    #[tokio::test]
    async fn system_runner_runs_a_real_command() {
        let output = SystemRunner.run("true", &[]).await;
        assert!(output.ok(), "{output:?}");
    }
}
