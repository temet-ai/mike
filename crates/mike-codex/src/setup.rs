use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const DEADLINE: Duration = Duration::from_secs(15);
const MAX_NOTICE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SetupOutcome {
    pub notice: String,
    pub mike_otel_exporter: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("could not run the Codex integration installer: {0}")]
    Io(#[from] std::io::Error),
    #[error("the Codex integration installer did not finish within 15 seconds")]
    Timeout,
    #[error("the Codex integration installer exceeded its 8 KiB diagnostic limit")]
    OutputLimit,
    #[error("the Codex integration installer returned an invalid setup report: {0}")]
    InvalidReport(#[from] serde_json::Error),
    #[error("the Codex integration installer failed ({code:?}): {detail}")]
    Failed { code: Option<i32>, detail: String },
}

pub async fn install(
    installer: &Path,
    codex_home: &Path,
    inspect_url: &str,
) -> Result<SetupOutcome, SetupError> {
    let mut child = Command::new(installer)
        .arg("--codex-home")
        .arg(codex_home)
        .arg("--mike-url")
        .arg(inspect_url)
        .arg("--require-mcp")
        .arg("--proxy-only")
        .arg("--report-json")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stderr = child.stderr.take().ok_or_else(|| {
        std::io::Error::other("the Codex integration installer has no diagnostic stream")
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("the Codex integration installer has no report stream"))?;
    let (report, notice, status) = tokio::time::timeout(DEADLINE, async {
        tokio::try_join!(read_notice(stdout), read_notice(stderr), async {
            child.wait().await.map_err(SetupError::Io)
        })
    })
    .await
    .map_err(|_| SetupError::Timeout)??;
    if !status.success() {
        return Err(SetupError::Failed {
            code: status.code(),
            detail: notice,
        });
    }
    let mut outcome: SetupOutcome = serde_json::from_str(&report)?;
    if !notice.is_empty() {
        outcome.notice = format!("{}\n{notice}", outcome.notice).trim().to_owned();
    }
    Ok(outcome)
}

async fn read_notice(stream: impl AsyncRead + Unpin) -> Result<String, SetupError> {
    let mut bytes = Vec::new();
    stream
        .take((MAX_NOTICE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > MAX_NOTICE_BYTES {
        return Err(SetupError::OutputLimit);
    }
    Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
