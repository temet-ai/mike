use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

use super::{NEVER_STARTED, START_GRACE_SECONDS, STOPPED, SeatStop, seat_exit};
use crate::base_state::BaseState;
use crate::launch::kill_for_server;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::UnsettledFleetLaunch;

pub(crate) async fn stop_seat(
    state: &BaseState,
    persist: &PersistHandle,
    run: &str,
    seat: &UnsettledFleetLaunch,
) -> SeatStop {
    let mut report = SeatStop {
        seat: seat.seat.clone(),
        session: seat.session_uuid,
        closed: false,
        note: None,
        launch_id: seat.id,
        process_exited: seat_exit::observe(persist, &state.config().inspect_listen_addr, run, seat)
            .await
            .unwrap_or(None),
    };
    if report.process_exited != Some(true) {
        match seat.session_uuid {
            Some(session) => match kill_for_server(state, session).await {
                Ok(killed) => {
                    report.closed = killed.closed;
                    report.process_exited =
                        seat_exit::observe(persist, &state.config().inspect_listen_addr, run, seat)
                            .await
                            .unwrap_or(None);
                    report.note = killed.note;
                }
                Err(error) => report.note = Some(error.to_string()),
            },
            None => report.note = Some("the launch recorded no session id".into()),
        }
    }
    if report.process_exited == Some(true) {
        if let Err(error) = persist
            .fleet()
            .record_fleet_launch_verified_exit(seat.id, run, STOPPED)
            .await
        {
            report.note = Some(format!(
                "root exit observed, but closure could not be recorded: {error}"
            ));
        } else {
            seat_exit::forget(run, seat);
        }
    } else if !close_if_never_started(persist, run, seat, &mut report).await {
        add_origin_diagnostic(&mut report);
    }
    report
}

async fn close_if_never_started(
    persist: &PersistHandle,
    run: &str,
    seat: &UnsettledFleetLaunch,
    report: &mut SeatStop,
) -> bool {
    if seat.opened_at.is_some() || seat.surface_ref.is_some() {
        return false;
    }
    match persist
        .launch()
        .close_never_started_fleet_launch(seat.id, run, NEVER_STARTED, START_GRACE_SECONDS)
        .await
    {
        Ok(true) => {
            report.closed = true;
            report.process_exited = Some(true);
            report.note = Some(format!(
                "{NEVER_STARTED}: the launch never recorded a pane or an open time, and its session has no request or has ended, so there is no process to wait for"
            ));
            true
        }
        Ok(false) => false,
        Err(error) => {
            report.note = Some(format!(
                "the launch looks never started, but closing it failed: {error}"
            ));
            false
        }
    }
}

fn add_origin_diagnostic(report: &mut SeatStop) {
    let detail = format!(
        "seat root exit is unverified ({}); observe the matching live session on its originating host; after restart, an already-ended root needs additional durable origin/exit evidence",
        crate::launch::local_root::capability()
    );
    report.note = Some(match report.note.take() {
        Some(note) => format!("{note}; {detail}"),
        None => detail,
    });
}
