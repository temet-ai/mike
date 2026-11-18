use std::fmt;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Program {
    Git,
    Gh,
    Cx,
}

impl Program {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Program::Git => "git",
            Program::Gh => "gh",
            Program::Cx => "scripts/cx",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Cmd {
    pub(crate) program: Program,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) timeout: Duration,
}

impl Cmd {
    pub(crate) fn new(program: Program, cwd: &Path, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cwd: cwd.to_path_buf(),
            timeout: Duration::from_secs(10 * 60),
        }
    }

    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) fn line(&self) -> String {
        format!("{} {}", self.program.name(), self.args.join(" "))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CmdOutput {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl CmdOutput {
    pub(crate) fn ok(&self) -> bool {
        self.code == Some(0)
    }

    pub(crate) fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr).trim().to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepError {
    pub(crate) step: String,
    pub(crate) code: Option<i32>,
    pub(crate) detail: String,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(code) => write!(f, "`{}` exited {code}: {}", self.step, self.detail),
            None => write!(f, "`{}` did not finish: {}", self.step, self.detail),
        }
    }
}

impl std::error::Error for StepError {}

pub(crate) trait CommandRunner: Send + Sync {
    fn run(&self, cmd: &Cmd) -> impl Future<Output = io::Result<CmdOutput>> + Send;
}

pub(crate) async fn output<R: CommandRunner>(
    runner: &R,
    cmd: &Cmd,
) -> Result<CmdOutput, StepError> {
    runner.run(cmd).await.map_err(|e| StepError {
        step: cmd.line(),
        code: None,
        detail: e.to_string(),
    })
}

pub(crate) async fn checked<R: CommandRunner>(runner: &R, cmd: &Cmd) -> Result<String, StepError> {
    let out = output(runner, cmd).await?;
    if out.ok() {
        return Ok(out.stdout.trim().to_owned());
    }
    Err(StepError {
        step: cmd.line(),
        code: out.code,
        detail: out.combined(),
    })
}

pub(crate) async fn git<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
    args: &[&str],
) -> Result<String, StepError> {
    checked(runner, &Cmd::new(Program::Git, cwd, args)).await
}

pub(crate) struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    async fn run(&self, cmd: &Cmd) -> io::Result<CmdOutput> {
        let program = match cmd.program {
            Program::Cx => cmd.cwd.join(cmd.program.name()).into_os_string(),
            other => other.name().into(),
        };
        let child = tokio::process::Command::new(program)
            .args(&cmd.args)
            .current_dir(&cmd.cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let out = tokio::time::timeout(cmd.timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("timed out after {} s", cmd.timeout.as_secs()),
                )
            })??;
        Ok(CmdOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}
