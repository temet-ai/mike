use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio::sync::Mutex;

const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
const CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexLogin {
    NotInstalled,
    SignedOut(String),
    SignedIn(String),
}

static CACHE: Mutex<Option<(Instant, CodexLogin)>> = Mutex::const_new(None);

pub fn classify(success: bool, stdout: &str, stderr: &str) -> CodexLogin {
    let said = [stdout, stderr]
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_owned();
    if success {
        CodexLogin::SignedIn(said)
    } else {
        CodexLogin::SignedOut(said)
    }
}

async fn probe(bin: &str) -> CodexLogin {
    let child = Command::new(bin)
        .args(["login", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return CodexLogin::NotInstalled,
        Err(e) => {
            return CodexLogin::SignedOut(format!("`codex login status` could not start: {e}"));
        }
    };
    match tokio::time::timeout(STATUS_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => classify(
            output.status.success(),
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        ),
        Ok(Err(e)) => CodexLogin::SignedOut(format!("`codex login status` failed: {e}")),
        Err(_) => CodexLogin::SignedOut(format!(
            "`{bin} login status` did not answer within {}s",
            STATUS_TIMEOUT.as_secs()
        )),
    }
}

pub async fn login_status(bin: &str) -> CodexLogin {
    let mut guard = CACHE.lock().await;
    if let Some((at, login)) = guard.as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return login.clone();
    }
    let fresh = probe(bin).await;
    *guard = Some((Instant::now(), fresh.clone()));
    fresh
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn classify_success_is_signed_in_with_the_cli_words() {
        assert_eq!(
            classify(true, "Logged in using ChatGPT\n", ""),
            CodexLogin::SignedIn("Logged in using ChatGPT".to_owned())
        );
    }

    #[test]
    fn classify_failure_is_signed_out_and_prefers_stdout_then_stderr() {
        assert_eq!(
            classify(false, "", "Not logged in\n"),
            CodexLogin::SignedOut("Not logged in".to_owned())
        );
        assert_eq!(
            classify(false, "Not logged in", "noise"),
            CodexLogin::SignedOut("Not logged in".to_owned())
        );
    }

    #[tokio::test]
    async fn probe_missing_binary_is_not_installed() {
        assert_eq!(
            probe("/nonexistent/mike-codex-binary").await,
            CodexLogin::NotInstalled
        );
    }
}
