use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use uuid::Uuid;

use crate::base_state::BaseState;
use crate::launch::kill_for_server;

const LAST_WORDS: Duration = Duration::from_secs(20);

pub fn is_open_run_seat(run: Option<&str>, seat: Option<&str>, closed: bool) -> bool {
    run.is_some() && seat.is_some() && !closed
}

pub fn close_later(state: &BaseState, session: Uuid) {
    let state = state.clone();
    state.tracker.clone().spawn(async move {
        let Some(persist) = state.persist() else {
            return;
        };
        let seat = match persist.launch().find_launch_by_session(session).await {
            Ok(Some(launch)) => is_open_run_seat(
                launch.run_id.as_deref(),
                launch.seat.as_deref(),
                launch.closed_at.is_some(),
            ),
            _ => false,
        };
        if !seat {
            return;
        }
        tokio::time::sleep(LAST_WORDS).await;
        match kill_for_server(&state, session).await {
            Ok(report) if report.closed => {
                tracing::info!(session_id = %session, "a run seat ended its registration, so mike closed its pane");
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(session_id = %session, error = %err, "mike could not close the pane of a run seat that ended its registration");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::is_open_run_seat;

    #[test]
    fn only_an_open_seat_of_a_run_has_its_pane_closed() {
        assert!(is_open_run_seat(Some("fleet-a-1"), Some("observer"), false));
        assert!(!is_open_run_seat(Some("fleet-a-1"), Some("observer"), true));
        assert!(!is_open_run_seat(None, Some("observer"), false));
        assert!(!is_open_run_seat(Some("fleet-a-1"), None, false));
    }
}
