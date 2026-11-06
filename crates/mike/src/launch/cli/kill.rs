use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ledger_wire::ClosedBody;
use super::runner::Runner;
use crate::launch::local_root::{self, RootKey};
use crate::launch::{CloseOutcome, LaunchError, Surface};
use crate::store::persistence::launch::LaunchRow;

const CLOSE_REASON: &str = "kill --session";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct KillReport {
    pub(crate) session: Uuid,
    pub(crate) surface: Option<String>,
    pub(crate) closed: bool,
    pub(crate) process_exited: Option<bool>,
    #[serde(skip_serializing, default)]
    pub(crate) note: Option<String>,
}

impl KillReport {
    pub(crate) fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!(
                "{{\"session\":\"{}\",\"closed\":false,\"error\":\"json encoding failed: {e}\"}}",
                self.session
            )
        })
    }

    fn no_op(session: Uuid, surface: Option<String>, note: impl Into<String>) -> Self {
        Self {
            session,
            surface,
            closed: false,
            process_exited: None,
            note: Some(note.into()),
        }
    }
}

pub(crate) async fn execute(runner: &Runner<'_>, session: Uuid) -> Result<KillReport, LaunchError> {
    let row = runner.ledger.find_launch_by_session(session).await;
    let launch_id = row.as_ref().map(|r| r.id);
    let recorded = row
        .as_ref()
        .and_then(|r| r.backend.parse::<crate::launch::BackendId>().ok())
        .filter(|id| *id != runner.backend.id())
        .and_then(crate::launch::backend_for);
    let backend = recorded.as_deref().unwrap_or(runner.backend);
    let Some(surface) = backend.find_by_session(session).await? else {
        return Ok(KillReport::no_op(
            session,
            None,
            "nothing to do: no surface hosts that session",
        ));
    };
    if let Some(detail) = mismatch(
        &surface,
        session,
        row.as_ref().and_then(|r| r.surface_ref.as_deref()),
    ) {
        return Ok(KillReport::no_op(
            session,
            Some(surface.surface_ref.clone()),
            format!("nothing closed: {detail}"),
        ));
    }
    if let Some(detail) = launch_mismatch(&surface, row.as_ref()) {
        return Ok(KillReport::no_op(
            session,
            Some(surface.surface_ref.clone()),
            detail,
        ));
    }
    let witness = launch_root(row.as_ref());
    if let Some(key) = &witness {
        local_root::bind_current(key, &surface);
    }
    let wait = Duration::from_secs(runner.config.kill_wait_secs);
    let report = match backend.close(&surface, wait).await? {
        CloseOutcome::Closed { process_exited } => KillReport {
            session,
            surface: Some(surface.surface_ref.clone()),
            closed: true,
            process_exited: exit_evidence(row.as_ref(), witness.as_ref(), &surface, process_exited),
            note: None,
        },
        CloseOutcome::NotFound => {
            return Ok(KillReport::no_op(
                session,
                Some(surface.surface_ref.clone()),
                "nothing to do: the surface is already gone",
            ));
        }
        CloseOutcome::Mismatch { detail } => {
            return Ok(KillReport::no_op(
                session,
                Some(surface.surface_ref.clone()),
                format!("nothing closed: {detail}"),
            ));
        }
    };
    let closed = ClosedBody {
        reason: CLOSE_REASON.to_string(),
        process_exited: report.process_exited,
    };
    runner.ledger.record_kill(launch_id, session, closed).await;
    Ok(report)
}

pub(super) fn launch_mismatch(surface: &Surface, row: Option<&LaunchRow>) -> Option<String> {
    let row = row?;
    if row.backend != surface.backend.as_str() {
        return Some("nothing closed: the current backend differs from the launch backend".into());
    }
    if let (Some(recorded), Some(current)) = (row.pid, surface.pid)
        && i32::try_from(current).ok() != Some(recorded)
        && !resumed_after_exit(row)
    {
        return Some(
            "nothing closed: the current session root differs from the recorded launch root".into(),
        );
    }
    None
}

fn resumed_after_exit(row: &LaunchRow) -> bool {
    row.closed_at.is_some() && row.close_process_exited == Some(true)
}

fn launch_root(row: Option<&LaunchRow>) -> Option<RootKey> {
    let row = row?;
    RootKey::from_record(
        row.id,
        row.run_id.as_deref()?,
        &row.backend,
        row.session_uuid,
        row.surface_ref.as_deref(),
        row.pid,
    )
}

fn exit_evidence(
    row: Option<&LaunchRow>,
    witness: Option<&RootKey>,
    surface: &Surface,
    backend_exited: bool,
) -> Option<bool> {
    if row.is_some_and(|row| row.run_id.is_some() && !resumed_after_exit(row)) {
        witness.and_then(local_root::observed_exit)
    } else {
        surface.pid.map(|_| backend_exited)
    }
}

pub(super) fn mismatch(surface: &Surface, session: Uuid, recorded: Option<&str>) -> Option<String> {
    match surface.session_uuid {
        Some(actual) if actual != session => {
            return Some(format!(
                "surface {} hosts session {actual}, not {session}",
                surface.surface_ref
            ));
        }
        None => {
            return Some(format!(
                "surface {} reports no session id; it cannot be confirmed as {session}",
                surface.surface_ref
            ));
        }
        Some(_) => {}
    }
    match recorded {
        Some(recorded) if recorded != surface.surface_ref => Some(format!(
            "the ledger recorded {session} on surface {recorded}, the backend finds it on {}",
            surface.surface_ref
        )),
        _ => None,
    }
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "kill_closure_cases.rs"]
pub(super) mod closure_cases;
