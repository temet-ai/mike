use std::path::Path;

use uuid::Uuid;

use super::Relaunch;
use crate::launch::channel_link;
use crate::state::AppState;
use crate::store::persistence::launch::{LaunchListFilter, LaunchRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Start {
    Resume,
    Fresh,
}

impl Start {
    pub(crate) fn flag(self) -> &'static str {
        match self {
            Self::Resume => "--resume",
            Self::Fresh => "--session-id",
        }
    }
}

pub(crate) fn fresh_args(args: &[String], session: Uuid, mcp_config: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--session-id" {
            iter.next();
            continue;
        }
        if !arg.starts_with("--session-id=") && !arg.starts_with("--mcp-config=") {
            out.push(arg.clone());
        }
    }
    out.push("--session-id".to_string());
    out.push(session.to_string());
    out.push(format!("--mcp-config={mcp_config}"));
    out
}

pub(crate) async fn start_for(state: &AppState, relaunch: &Relaunch, session: Uuid) -> Start {
    if transcript_written(Path::new(&relaunch.folder), session)
        || made_requests(state, relaunch, session).await
    {
        Start::Resume
    } else {
        Start::Fresh
    }
}

fn transcript_written(folder: &Path, session: Uuid) -> bool {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .is_some_and(|home| {
            channel_link::transcript_path(Path::new(&home), folder, session).exists()
        })
}

async fn made_requests(state: &AppState, relaunch: &Relaunch, session: Uuid) -> bool {
    let (Some(persist), Some(run)) = (state.persist(), relaunch.run.as_deref()) else {
        return false;
    };
    persist.fleet_seat_rows(run).await.is_ok_and(|rows| {
        rows.iter()
            .any(|row| row.session_uuid == Some(session) && row.requests > 0)
    })
}

pub(crate) async fn replaced(state: &AppState, relaunch: &Relaunch, session: Uuid) -> bool {
    let (Some(persist), Some(run), Some(seat)) = (
        state.persist(),
        relaunch.run.as_deref(),
        relaunch.seat.as_deref(),
    ) else {
        return false;
    };
    let launches = persist
        .list_launches(&LaunchListFilter {
            run_id: Some(run.to_string()),
            open_only: false,
            backend: None,
            limit: LaunchListFilter::DEFAULT_LIMIT,
        })
        .await
        .unwrap_or_default();
    later_seat_launch(&launches, seat, session)
}

pub(crate) fn later_seat_launch(launches: &[LaunchRow], seat: &str, session: Uuid) -> bool {
    let Some(first) = launches
        .iter()
        .filter(|row| row.session_uuid == Some(session))
        .map(|row| row.created_at)
        .min()
    else {
        return false;
    };
    launches.iter().any(|row| {
        row.seat.as_deref() == Some(seat)
            && row.session_uuid.is_some_and(|other| other != session)
            && row.created_at > first
    })
}
