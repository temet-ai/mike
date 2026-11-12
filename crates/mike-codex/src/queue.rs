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
        .args(["queue", "--thread", thread, "--message", notice, "--resume"])
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

pub async fn local_thread_exists(home: &Path, thread: &str) -> std::io::Result<bool> {
    if thread.is_empty() || thread.len() > 128 || thread.contains(['/', '\\']) {
        return Ok(false);
    }
    let suffix = format!("{thread}.jsonl");
    let mut directories = vec![
        (home.join("sessions"), 0),
        (home.join("archived_sessions"), 0),
    ];
    let mut visited = 0;
    while let Some((directory, depth)) = directories.pop() {
        match scan_directory(&directory, depth, &suffix, &mut directories, &mut visited).await? {
            Scan::Found => return Ok(true),
            Scan::Limit => return Ok(false),
            Scan::Next => {}
        }
    }
    Ok(false)
}

enum Scan {
    Found,
    Limit,
    Next,
}

async fn scan_directory(
    directory: &Path,
    depth: u8,
    suffix: &str,
    directories: &mut Vec<(std::path::PathBuf, u8)>,
    visited: &mut usize,
) -> std::io::Result<Scan> {
    let mut entries = match tokio::fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Scan::Next),
        Err(error) => return Err(error),
    };
    while let Some(entry) = entries.next_entry().await? {
        *visited += 1;
        if *visited > 20_000 {
            return Ok(Scan::Limit);
        }
        let kind = entry.file_type().await?;
        if kind.is_file() && entry.file_name().to_string_lossy().ends_with(suffix) {
            return Ok(Scan::Found);
        }
        if kind.is_dir() && depth < 3 {
            directories.push((entry.path(), depth + 1));
        }
    }
    Ok(Scan::Next)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[tokio::test]
    async fn local_queue_exit_means_queued_never_consumed() -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let program = home.path().join("codex");
        tokio::fs::write(&program, b"#!/bin/sh\n[ \"$1\" = queue ] && [ \"$2\" = --thread ] && [ \"$4\" = --message ] && [ \"$6\" = --resume ]\n").await?;
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

    #[tokio::test]
    async fn queue_requires_existing_local_thread_and_rejects_path_identifiers()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let id = "00000000-0000-0000-0000-000000000001";
        assert!(!local_thread_exists(home.path(), id).await?);
        assert!(!local_thread_exists(home.path(), "../../outside").await?);
        let directory = home.path().join("sessions/2026/11/12");
        tokio::fs::create_dir_all(&directory).await?;
        tokio::fs::write(directory.join(format!("rollout-test-{id}.jsonl")), b"").await?;
        assert!(local_thread_exists(home.path(), id).await?);
        Ok(())
    }
}
