use std::path::Path;
use std::time::Duration;

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv};
use super::surface::pane_at_shell_prompt;
use crate::launch::error::LaunchError;
use crate::launch::fleet::seat_helpers;

const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(100);

struct Receipt {
    directory: tempfile::TempDir,
    nonce: String,
}

impl Receipt {
    fn new(directory: &Path) -> Result<Self, LaunchError> {
        Ok(Self {
            directory: tempfile::Builder::new()
                .prefix("activation-")
                .tempdir_in(directory)
                .map_err(|error| {
                    LaunchError::io("preparing fleet helper activation receipt", error)
                })?,
            nonce: uuid::Uuid::new_v4().to_string(),
        })
    }

    fn command(&self, setup: &str) -> String {
        let receipt = self.directory.path().join("ready");
        format!(
            "{setup} && printf '%s' '{}' > '{}'",
            self.nonce,
            receipt.to_string_lossy().replace('\'', "'\\''")
        )
    }

    fn ready(&self) -> Result<bool, LaunchError> {
        match std::fs::read(self.directory.path().join("ready")) {
            Ok(bytes) if bytes.is_empty() => Ok(false),
            Ok(bytes) if bytes == self.nonce.as_bytes() => Ok(true),
            Ok(_) => Err(LaunchError::Failed(
                "fleet helper activation receipt mismatched; refusing agent start".into(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(LaunchError::io(
                "reading fleet helper activation receipt",
                error,
            )),
        }
    }
}

pub async fn prepare(
    cli: &HerdrCli,
    args: &[String],
    pane: &str,
) -> Result<(), LaunchError> {
    prepare_with_wait(cli, args, pane, WAIT, POLL).await
}

async fn prepare_with_wait(
    cli: &HerdrCli,
    args: &[String],
    pane: &str,
    wait: Duration,
    poll: Duration,
) -> Result<(), LaunchError> {
    let Some(setup) = seat_helpers::setup(args).map_err(LaunchError::Failed)? else {
        return Ok(());
    };
    let receipt = Receipt::new(&setup.directory)?;
    let deadline = tokio::time::Instant::now() + wait;
    tokio::time::timeout_at(deadline, async {
        while !pane_at_shell_prompt(cli, pane).await? {
            wait_next(deadline, poll).await?;
        }
        cli.run_json(
            &argv(&["pane", "run", pane, &receipt.command(&setup.command)]),
            DEFAULT_TIMEOUT,
        )
        .await?;
        while !receipt.ready()? {
            wait_next(deadline, poll).await?;
        }
        Ok(())
    })
    .await
    .map_err(|_| unconfirmed())?
}

async fn wait_next(deadline: tokio::time::Instant, poll: Duration) -> Result<(), LaunchError> {
    if tokio::time::Instant::now() >= deadline {
        return Err(unconfirmed());
    }
    tokio::time::sleep_until((tokio::time::Instant::now() + poll).min(deadline)).await;
    Ok(())
}

fn unconfirmed() -> LaunchError {
    LaunchError::Failed("fleet helper activation was not confirmed; refusing agent start".into())
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "browser_guard_cases.rs"]
pub mod cases;

#[cfg(test)]
#[path = "browser_guard_tests.rs"]
mod tests;
