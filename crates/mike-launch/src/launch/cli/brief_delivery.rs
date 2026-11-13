//! Delivering a note-mode brief after the surface opens.
//!
//! A note brief is not typed. Once the channel check has linked the session's
//! Remote Control stream, the launcher asks mike to mail the brief and push
//! it over Remote Control (`POST /api/launches/{session}/brief`). Claude Code
//! frames that push as a teammate's request, so the idle session starts on
//! it. When nothing was pushed (no stream, no proxy, a failed call), the
//! launcher types the pointer instead: the operator's own first message
//! telling the session to read its brief note.

use uuid::Uuid;

use super::delivery::PROMPT_WAIT_MS;
use super::runner::Runner;
use crate::launch::{BriefMode, ChannelCheck, LaunchSpec};

/// What happened to a note brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteBriefOutcome {
    Pushed,
    Claimed,
    PointerTyped { confirmed: bool },
    Undelivered,
}

impl NoteBriefOutcome {
    pub fn delivered(self) -> bool {
        !matches!(self, Self::Undelivered)
    }

    pub fn confirmed(self) -> bool {
        matches!(
            self,
            Self::Pushed | Self::Claimed | Self::PointerTyped { confirmed: true }
        )
    }
}

/// `None` unless the spec carries a note-mode brief for a known session.
pub async fn deliver_note_brief(
    runner: &Runner<'_>,
    spec: &LaunchSpec,
    check: ChannelCheck,
) -> Option<NoteBriefOutcome> {
    let brief = spec.brief.as_ref().filter(|b| b.mode == BriefMode::Note)?;
    let session = spec.session?;
    if check == ChannelCheck::Found
        && push_over_remote_control(runner.inspect_addr.as_deref(), session).await
    {
        eprintln!("mike spawn: brief pushed to {session} over Remote Control");
        return Some(NoteBriefOutcome::Pushed);
    }
    if already_read(runner, session).await {
        eprintln!(
            "mike spawn: {session} has already taken its brief (a client claimed it); no pointer typed"
        );
        return Some(NoteBriefOutcome::Claimed);
    }
    eprintln!("mike spawn: brief not pushed to {session}; typing the pointer to its note");
    Some(
        match runner
            .backend
            .send_prompt(&spec.agent_name, &brief.delivered, PROMPT_WAIT_MS)
            .await
        {
            Ok(confirmed) => NoteBriefOutcome::PointerTyped { confirmed },
            Err(e) => {
                eprintln!("mike spawn: could not type the brief pointer: {e}");
                NoteBriefOutcome::Undelivered
            }
        },
    )
}

async fn already_read(runner: &Runner<'_>, session: Uuid) -> bool {
    runner
        .ledger
        .find_launch_by_session(session)
        .await
        .is_some_and(|row| row.brief_read_at.is_some())
}

#[derive(serde::Deserialize)]
struct Delivery {
    pushed: usize,
}

async fn push_over_remote_control(inspect_addr: Option<&str>, session: Uuid) -> bool {
    let Some(addr) = inspect_addr else {
        return false;
    };
    let url = format!("http://{addr}/api/launches/{session}/brief");
    let response = match reqwest::Client::new().post(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            eprintln!(
                "mike spawn: brief push for {session} answered {}",
                r.status()
            );
            return false;
        }
        Err(e) => {
            eprintln!("mike spawn: brief push for {session} failed: {e}");
            return false;
        }
    };
    response
        .json::<Delivery>()
        .await
        .is_ok_and(|d| d.pushed > 0)
}
