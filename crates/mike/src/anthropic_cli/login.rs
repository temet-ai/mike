use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::cli::AnthropicCli;
use super::env::clean_cli_env;
use super::error::AnthropicCliError;
use super::login_reader::run_login_reader;

pub(crate) const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum LoginPhase {
    Starting,
    AwaitingCode { url: String },
    Succeeded,
    Failed { message: String },
    Cancelled,
}

impl LoginPhase {
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed { .. } | Self::Cancelled
        )
    }
}

pub(crate) struct LoginInner {
    pub(crate) phase: LoginPhase,
    pub(crate) stdin: Option<ChildStdin>,
}

pub(crate) struct LoginSession {
    pub(crate) id: String,
    pub(crate) inner: Arc<Mutex<LoginInner>>,
    cancel: CancellationToken,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LoginStatusDto {
    pub(crate) id: String,
    #[serde(flatten)]
    pub(crate) phase: LoginPhase,
}

impl LoginSession {
    pub(crate) async fn status(&self) -> LoginStatusDto {
        LoginStatusDto {
            id: self.id.clone(),
            phase: self.inner.lock().await.phase.clone(),
        }
    }

    pub(crate) async fn is_terminal(&self) -> bool {
        self.inner.lock().await.phase.is_terminal()
    }

    pub(crate) async fn submit_code(&self, code: &str) -> Result<(), AnthropicCliError> {
        let mut guard = self.inner.lock().await;
        let Some(stdin) = guard.stdin.as_mut() else {
            return Err(AnthropicCliError::Failed(
                "sign-in process is no longer accepting input".to_owned(),
            ));
        };
        let line = format!("{}\n", code.trim());
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| AnthropicCliError::Io(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| AnthropicCliError::Io(e.to_string()))
    }

    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }
}

pub(crate) fn spawn_login(cli: &AnthropicCli) -> Result<LoginSession, AnthropicCliError> {
    let env = clean_cli_env();
    let mut command = tokio::process::Command::new(cli.bin());
    command.args(["auth", "login"]);
    command.env_clear();
    command.envs(&env);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);

    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AnthropicCliError::NotFound
        } else {
            AnthropicCliError::Io(e.to_string())
        }
    })?;

    let stdin = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AnthropicCliError::Failed("no stdout on spawned process".to_owned()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let inner = Arc::new(Mutex::new(LoginInner {
        phase: LoginPhase::Starting,
        stdin,
    }));
    let cancel = CancellationToken::new();

    tokio::spawn(run_login_reader(
        child,
        stdout,
        inner.clone(),
        cancel.clone(),
    ));

    Ok(LoginSession { id, inner, cancel })
}
