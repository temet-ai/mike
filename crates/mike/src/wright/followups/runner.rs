use std::future::Future;
use std::io;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;

pub(crate) use crate::wright::repair::runner::{CmdOutput, StepError};

const SSH_OPTIONS: [&str; 4] = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Program {
    Gh,
    Git,
    Ssh,
    Mike,
}

impl Program {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Program::Gh => "gh",
            Program::Git => "git",
            Program::Ssh => "ssh",
            Program::Mike => "mike",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Cmd {
    pub(crate) program: Program,
    pub(crate) args: Vec<String>,
    pub(crate) stdin: Option<String>,
    pub(crate) timeout: Duration,
}

impl Cmd {
    pub(crate) fn new(program: Program, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            stdin: None,
            timeout: Duration::from_secs(60),
        }
    }

    pub(crate) fn remote(host: &str, program: Program, args: &[&str]) -> Self {
        let line = std::iter::once(program.name().to_owned())
            .chain(args.iter().map(|arg| sh_quote(arg)))
            .collect::<Vec<_>>()
            .join(" ");
        Self::remote_line(host, &line)
    }

    pub(crate) fn remote_line(host: &str, line: &str) -> Self {
        let mut args: Vec<&str> = SSH_OPTIONS.to_vec();
        args.extend(["--", host, line]);
        Self::new(Program::Ssh, &args)
    }

    pub(crate) fn with_stdin(mut self, stdin: String) -> Self {
        self.stdin = Some(stdin);
        self
    }

    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) fn line(&self) -> String {
        format!("{} {}", self.program.name(), self.args.join(" "))
    }
}

pub(crate) fn sh_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

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

pub(crate) struct ProcessRunner;

async fn feed(stdin: Option<tokio::process::ChildStdin>, text: Option<&str>) -> io::Result<()> {
    let (Some(mut pipe), Some(text)) = (stdin, text) else {
        return Ok(());
    };
    pipe.write_all(text.as_bytes()).await?;
    pipe.shutdown().await
}

impl CommandRunner for ProcessRunner {
    async fn run(&self, cmd: &Cmd) -> io::Result<CmdOutput> {
        let stdin = if cmd.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        };
        let mut child = tokio::process::Command::new(cmd.program.name())
            .args(&cmd.args)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let pipe = child.stdin.take();
        let run = async {
            let (fed, out) =
                tokio::join!(feed(pipe, cmd.stdin.as_deref()), child.wait_with_output());
            fed.and(out)
        };
        let out = tokio::time::timeout(cmd.timeout, run).await.map_err(|_| {
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
