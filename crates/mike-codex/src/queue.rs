use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueOutcome {
    Queued,
    NotStarted,
    Uncertain,
}

pub async fn queue_notification(
    program: &Path,
    home: &Path,
    thread: &str,
    notice: &str,
    deadline: Duration,
) -> QueueOutcome {
    let child = Command::new(program)
        .args(["queue", "--thread", thread, "--message", notice])
        .env("CODEX_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let Ok(mut child) = child else {
        return QueueOutcome::NotStarted;
    };
    match tokio::time::timeout(deadline, child.wait()).await {
        Ok(Ok(status)) if status.success() => QueueOutcome::Queued,
        _ => QueueOutcome::Uncertain,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[tokio::test]
    async fn queue_notification_uses_the_existing_owner_without_daemon_resume() -> anyhow::Result<()>
    {
        let home = tempfile::tempdir()?;
        let program = home.path().join("codex");
        tokio::fs::write(&program, b"#!/bin/sh\n[ \"$#\" = 5 ] && [ \"$1\" = queue ] && [ \"$2\" = --thread ] && [ \"$3\" = thread ] && [ \"$4\" = --message ] && [ \"$5\" = 'static notice' ]\n").await?;
        tokio::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).await?;
        assert_eq!(
            queue_notification(
                &program,
                home.path(),
                "thread",
                "static notice",
                Duration::from_secs(2)
            )
            .await,
            QueueOutcome::Queued
        );
        assert_eq!(
            queue_notification(
                &home.path().join("missing"),
                home.path(),
                "thread",
                "notice",
                Duration::from_secs(2)
            )
            .await,
            QueueOutcome::NotStarted
        );
        tokio::fs::write(&program, b"#!/bin/sh\nexit 1\n").await?;
        assert_eq!(
            queue_notification(
                &program,
                home.path(),
                "thread",
                "notice",
                Duration::from_secs(2)
            )
            .await,
            QueueOutcome::Uncertain
        );
        Ok(())
    }
}
