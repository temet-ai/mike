use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;

const TURN_DEADLINE: Duration = Duration::from_secs(600);

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new() -> anyhow::Result<Self> {
        let root = std::env::temp_dir().join(format!("mike-codex-seat-{}", uuid::Uuid::new_v4()));
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo)?;
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()?;
        anyhow::ensure!(status.success(), "git init failed in {}", repo.display());
        std::fs::write(repo.join("check.sh"), "#!/bin/sh\ngrep -q done note.txt\n")?;
        std::process::Command::new("chmod")
            .args(["755", "check.sh"])
            .current_dir(&repo)
            .status()?;
        Ok(Self { root })
    }

    fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    fn options(&self, home: &Path) -> SeatOptions {
        SeatOptions {
            program: "codex".into(),
            cwd: self.repo(),
            codex_home: home.to_path_buf(),
            model: None,
            effort: None,
            developer_instructions: render_contract(
                SeatKind::Worker,
                "codex-P1",
                uuid::Uuid::new_v4(),
                "Do exactly what each message asks, in this repository.",
            ),
            mcp: None,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
#[ignore = "runs the installed codex CLI; no login needed"]
async fn codex_seat_starts_a_persistent_thread_and_stops_without_login() -> anyhow::Result<()> {
    let scratch = Scratch::new()?;
    let home = scratch.root.join("codex-home");
    super::options::prepare_home(&home, None)?;
    let seat = CodexSeat::launch(scratch.options(&home)).await?;
    assert!(!seat.thread_id().trim().is_empty());
    let pid = seat
        .pid()
        .ok_or_else(|| anyhow::anyhow!("no app-server pid"))?;
    seat.stop().await?;
    assert!(!Path::new(&format!("/proc/{pid}/cmdline")).exists());
    Ok(())
}

#[tokio::test]
#[ignore = "runs the installed codex CLI with the operator's login: a real edit, check, restart and fork"]
async fn codex_seat_edits_checks_restarts_resumes_and_forks() -> anyhow::Result<()> {
    let scratch = Scratch::new()?;
    let home = scratch.root.join("codex-home");
    super::options::prepare_home(&home, super::options::operator_codex_home().as_deref())?;
    let mut seat = CodexSeat::launch(scratch.options(&home)).await?;
    let turn = seat
        .brief("Create note.txt containing the word done, then run ./check.sh and report its exit code.")
        .await?;
    let first = seat.wait_turn(&turn, TURN_DEADLINE).await?;
    assert_eq!(first.status, "completed");
    assert_eq!(
        std::fs::read_to_string(scratch.repo().join("note.txt"))?.trim(),
        "done"
    );
    assert!(
        first
            .commands
            .iter()
            .any(|(c, code)| c.contains("check.sh") && *code == Some(0))
    );
    let thread = seat.thread_id().to_string();
    seat.stop().await?;

    let mut resumed = CodexSeat::resume(scratch.options(&home), &thread).await?;
    assert_eq!(resumed.thread_id(), thread);
    let turn = resumed
        .brief("Which file did you create in your previous turn? Answer with the file name only.")
        .await?;
    let second = resumed.wait_turn(&turn, TURN_DEADLINE).await?;
    assert!(second.final_text.contains("note.txt"), "{second:?}");
    resumed.stop().await?;

    let forked = CodexSeat::fork(scratch.options(&home), &thread).await?;
    assert_ne!(forked.thread_id(), thread);
    assert_eq!(forked.forked_from(), Some(thread.as_str()));
    forked.stop().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "runs the installed codex CLI with the operator's login: interrupt a running turn"]
async fn codex_seat_interrupts_a_running_turn() -> anyhow::Result<()> {
    let scratch = Scratch::new()?;
    let home = scratch.root.join("codex-home");
    super::options::prepare_home(&home, super::options::operator_codex_home().as_deref())?;
    let mut seat = CodexSeat::launch(scratch.options(&home)).await?;
    let turn = seat
        .brief("Run `sleep 120` in the shell, then say finished.")
        .await?;
    while let Some(event) = seat.next_event().await {
        if event["method"] == "item/started"
            && event["params"]["item"]["type"] == "commandExecution"
        {
            break;
        }
    }
    seat.interrupt(&turn).await?;
    let outcome = seat.wait_turn(&turn, TURN_DEADLINE).await?;
    assert_eq!(outcome.status, "interrupted");
    seat.stop().await?;
    Ok(())
}
