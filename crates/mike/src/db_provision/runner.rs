use std::future::Future;
use std::io;
use std::process::Stdio;

use super::DbError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl Call {
    pub(crate) fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(ToString::to_string).collect(),
            env: Vec::new(),
        }
    }

    pub(crate) fn with_env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_string(), value.to_string()));
        self
    }

    pub(crate) fn shown(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Captured {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Captured {
    pub(crate) fn detail(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        text.trim().to_string()
    }
}

pub(crate) trait Runner {
    fn run(&self, call: &Call) -> impl Future<Output = io::Result<Captured>> + Send;
}

pub(crate) struct SystemRunner;

pub(crate) fn sandbox_guard(sandbox_active: bool) -> io::Result<()> {
    if sandbox_active {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "running docker, brew, launchctl and systemctl is switched off while {} is set",
                crate::sandbox::ENV_VAR
            ),
        ));
    }
    Ok(())
}

impl Runner for SystemRunner {
    async fn run(&self, call: &Call) -> io::Result<Captured> {
        sandbox_guard(crate::sandbox::active())?;
        let output = tokio::process::Command::new(&call.program)
            .args(&call.args)
            .envs(call.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await?;
        Ok(Captured {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

pub(crate) fn spawn_error(call: &Call, source: io::Error) -> DbError {
    if call.program == "docker" && source.kind() == io::ErrorKind::NotFound {
        return DbError::DockerMissing;
    }
    DbError::Spawn {
        program: call.program.clone(),
        source,
    }
}

pub(crate) async fn run_loose<R: Runner>(runner: &R, call: &Call) -> Result<Captured, DbError> {
    runner
        .run(call)
        .await
        .map_err(|source| spawn_error(call, source))
}

pub(crate) async fn run_checked<R: Runner>(runner: &R, call: &Call) -> Result<Captured, DbError> {
    let captured = run_loose(runner, call).await?;
    if captured.success {
        Ok(captured)
    } else {
        Err(DbError::CommandFailed {
            command: call.shown(),
            detail: captured.detail(),
        })
    }
}
