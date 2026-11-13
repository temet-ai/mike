use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::login::{LOGIN_TIMEOUT, LoginInner, LoginPhase};

fn extract_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

async fn set_phase(inner: &Arc<Mutex<LoginInner>>, phase: LoginPhase) {
    let mut guard = inner.lock().await;
    if !guard.phase.is_terminal() {
        guard.phase = phase;
    }
}

async fn set_url_if_starting(inner: &Arc<Mutex<LoginInner>>, url: String) {
    let mut guard = inner.lock().await;
    if matches!(guard.phase, LoginPhase::Starting) {
        guard.phase = LoginPhase::AwaitingCode { url };
    }
}

enum ReadOutcome {
    Cancelled,
    TimedOut,
    Closed,
}

async fn read_until_closed(
    stdout: ChildStdout,
    inner: &Arc<Mutex<LoginInner>>,
    cancel: &CancellationToken,
) -> ReadOutcome {
    let mut lines = BufReader::new(stdout).lines();
    let deadline = tokio::time::sleep(LOGIN_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return ReadOutcome::Cancelled,
            _ = &mut deadline => return ReadOutcome::TimedOut,
            line = lines.next_line() => match line {
                Ok(Some(text)) => {
                    if let Some(url) = extract_url(&text) {
                        set_url_if_starting(inner, url).await;
                    }
                }
                Ok(None) | Err(_) => return ReadOutcome::Closed,
            },
        }
    }
}

async fn finish_after_exit(mut child: Child, inner: &Arc<Mutex<LoginInner>>) {
    let phase = match child.wait().await {
        Ok(status) if status.success() => LoginPhase::Succeeded,
        Ok(status) => LoginPhase::Failed {
            message: format!("claude auth login exited with {status}"),
        },
        Err(e) => LoginPhase::Failed {
            message: format!("could not wait for claude auth login: {e}"),
        },
    };
    set_phase(inner, phase).await;
}

pub async fn run_login_reader(
    mut child: Child,
    stdout: ChildStdout,
    inner: Arc<Mutex<LoginInner>>,
    cancel: CancellationToken,
) {
    match read_until_closed(stdout, &inner, &cancel).await {
        ReadOutcome::Cancelled => {
            let _ = child.kill().await;
            set_phase(&inner, LoginPhase::Cancelled).await;
        }
        ReadOutcome::TimedOut => {
            let _ = child.kill().await;
            set_phase(
                &inner,
                LoginPhase::Failed {
                    message: "sign-in timed out after 10 minutes".to_owned(),
                },
            )
            .await;
        }
        ReadOutcome::Closed => finish_after_exit(child, &inner).await,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn extract_url_finds_https_and_stops_at_whitespace() {
        let line = "If the browser didn't open, visit: https://claude.com/oauth?x=1 trailing";
        assert_eq!(
            extract_url(line).as_deref(),
            Some("https://claude.com/oauth?x=1")
        );
    }

    #[test]
    fn extract_url_none_when_no_https() {
        assert!(extract_url("Opening browser to sign in…").is_none());
    }

    #[test]
    fn extract_url_takes_full_remainder_when_no_trailing_whitespace() {
        assert_eq!(
            extract_url("https://claude.com/oauth?x=1").as_deref(),
            Some("https://claude.com/oauth?x=1")
        );
    }
}
