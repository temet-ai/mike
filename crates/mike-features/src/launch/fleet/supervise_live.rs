use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::Value;
use uuid::Uuid;

use super::facts::{Observed, STOP_KILL, seat_facts};
use super::{Action, Effects, SeatFacts, SystemClock, finished_tasks, payload, run};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::events::{
    EventPayload, FleetEvent, NewFleetEvent, SeatClosed, SeatFirstRequest, SeatLaunchConfirmed,
};
use crate::launch::fleet::plan::ControlSeats;
use crate::launch::fleet::shutdown::STOPPING;
use crate::launch::fleet::status::FleetStatus;
use crate::launch::kill_for_server;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetRow, UnsettledFleetLaunch};
use crate::store::persistence::session_notes::NewSessionNote;
use crate::types::SortOrder;

struct Live<'a> {
    state: &'a AppState,
    persist: &'a PersistHandle,
    run: &'a str,
    fleet: Uuid,
    coordinator: Option<Uuid>,
}

impl Live<'_> {
    async fn record(&self, task: Option<&str>, session: Option<Uuid>, body: EventPayload) {
        let event = NewFleetEvent {
            run_id: self.run.to_string(),
            task: task.map(str::to_string),
            session_uuid: session,
            payload: body,
        };
        if let Err(err) = self.persist.fleet().append_fleet_event(&event).await {
            tracing::warn!(error = %err, run = %self.run, "fleet supervisor: an event could not be recorded");
        }
    }

    async fn close(&self, task: &str, session: Uuid, reason: &str) {
        let body = EventPayload::SeatClosed(SeatClosed {
            reason: reason.to_string(),
        });
        self.record(Some(task), Some(session), body).await;
    }

    async fn clear_seat(&self, task: &str, session: Uuid, state: String) -> Result<(), String> {
        tracing::info!(task, session_id = %session, %state, "fleet supervisor: the seat is closed in the event record; staffing seats the task again");
        Ok(())
    }

    async fn needs_you(&self, task: Option<&str>, words: &str) -> Result<(), String> {
        let Some(coordinator) = self.coordinator else {
            return Err("the run records no coordinator to hold the note".to_string());
        };
        let title = super::needs_you_title(task);
        let tags = [format!("fleet:{}", self.fleet)];
        let note = NewSessionNote {
            kind: Some("note"),
            title: &title,
            body: Some(words),
            status: Some("operator"),
            tags: Some(&tags),
            data: None,
        };
        self.persist
            .launch()
            .create_session_note(coordinator, note)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    async fn wake(&self, session: Uuid) -> Result<(), String> {
        let addr = &self.state.config.inspect_listen_addr;
        let url = format!("http://{addr}/api/launches/{session}/brief");
        let response = reqwest::Client::new()
            .post(&url)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        let body: Value = response.json().await.map_err(|err| err.to_string())?;
        if body["pushed"].as_u64().unwrap_or(0) > 0 {
            Ok(())
        } else {
            Err(format!("Remote Control pushed nothing to {session}"))
        }
    }
}

#[async_trait::async_trait]
impl Effects for Live<'_> {
    async fn apply(&self, action: &Action) -> Result<(), String> {
        match action {
            Action::KillSeat { session, .. } => kill_for_server(self.state, *session)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string()),
            Action::StopKill { task, session } => {
                let killed = kill_for_server(self.state, *session).await;
                self.close(task, *session, STOP_KILL).await;
                killed.map(|_| ()).map_err(|err| err.to_string())
            }
            Action::Relaunch {
                task,
                session,
                attempt,
            } => {
                self.close(task, *session, "launch not confirmed; relaunched")
                    .await;
                let state = format!(
                    "relaunching (attempt {attempt} of {}): the seat made no first request in time",
                    super::RELAUNCHES
                );
                self.clear_seat(task, *session, state).await
            }
            Action::Replace {
                task,
                session,
                reason,
            } => {
                self.close(task, *session, reason).await;
                self.clear_seat(task, *session, format!("replaced: {reason}"))
                    .await
            }
            Action::RouteSuspect {
                task,
                session,
                words,
            } => {
                self.close(task, *session, "launch route suspect").await;
                self.needs_you(Some(task), words).await
            }
            Action::NeedsYou { task, words } => self.needs_you(task.as_deref(), words).await,
            Action::Wake { task, session } => {
                self.wake(*session).await?;
                if let Some(body) = payload(action) {
                    self.record(Some(task), Some(*session), body).await;
                }
                Ok(())
            }
            Action::RecordLaunchFailed { task, session, .. }
            | Action::RecordProcessGone { task, session, .. }
            | Action::RecordUnkillable { task, session } => {
                if let Some(body) = payload(action) {
                    self.record(Some(task), Some(*session), body).await;
                }
                Ok(())
            }
            Action::KillCheck { .. }
            | Action::RequeueCheck { .. }
            | Action::FailCheck { .. }
            | Action::SlotSuspect { .. } => Ok(()),
        }
    }
}

async fn observed(
    persist: &PersistHandle,
    run: &str,
    session: Uuid,
    launches: &[UnsettledFleetLaunch],
    stopping: bool,
) -> Observed {
    let db_id = persist
        .resolve_session_db_id(&session.to_string())
        .await
        .ok()
        .flatten();
    let last_seen = match db_id {
        Some(id) => persist
            .steerer()
            .session_last_request_at(id)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let unread_mail = persist
        .launch()
        .has_pending_inbound_mail(session)
        .await
        .unwrap_or(false);
    let mut process_gone = false;
    for seat in launches
        .iter()
        .filter(|l| l.opened && l.session_uuid == Some(session))
    {
        if matches!(
            crate::launch::fleet::shutdown::seat_exit::observe(persist, run, seat).await,
            Ok(Some(true))
        ) {
            process_gone = true;
        }
    }
    Observed {
        last_seen,
        unread_mail,
        process_gone,
        stopping,
        confirmed_by_request: last_seen.is_some(),
    }
}

async fn confirm(live: &Live<'_>, seat: &SeatFacts, events: &[FleetEvent]) {
    let already = events.iter().any(|e| {
        e.session_uuid == Some(seat.session)
            && e.kind == crate::launch::fleet::events::EventKind::SeatLaunchConfirmed
    });
    if already {
        return;
    }
    let first = match live
        .persist
        .resolve_session_db_id(&seat.session.to_string())
        .await
    {
        Ok(Some(id)) => live
            .persist
            .fetch_session_request_summaries_paginated(id, 0, 1, SortOrder::Asc)
            .await
            .ok()
            .and_then(|(rows, _)| rows.first().map(|r| r.id)),
        _ => None,
    };
    let Some(request) = first else { return };
    let (task, session) = (Some(seat.task.as_str()), Some(seat.session));
    live.record(
        task,
        session,
        EventPayload::SeatFirstRequest(SeatFirstRequest {
            request_id: i64::try_from(request).unwrap_or(0),
        }),
    )
    .await;
    live.record(
        task,
        session,
        EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed {
            launch_id: seat.launch_id,
        }),
    )
    .await;
}

pub async fn pass(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    status: &FleetStatus,
) -> Vec<Action> {
    let Some(run_id) = row.run_id.as_deref() else {
        return Vec::new();
    };
    let finished = finished_tasks(status);
    let Ok(events) = persist.fleet().fleet_events_since(run_id, 0).await else {
        return Vec::new();
    };
    let launches = persist
        .fleet()
        .unsettled_fleet_launches(run_id)
        .await
        .unwrap_or_default();
    let live = Live {
        state,
        persist,
        run: run_id,
        fleet: row.fleet_uuid,
        coordinator: ControlSeats::recorded(def).map(|c| c.coordinator),
    };
    let stopping = row.state == STOPPING;
    let mut seats = Vec::new();
    for task in def.tasks.keys() {
        let Some(session) =
            crate::launch::fleet::definition::task_run(status, task).and_then(|run| run.session)
        else {
            continue;
        };
        if finished.contains(task) && !stopping {
            continue;
        }
        let seen = observed(persist, run_id, session, &launches, stopping).await;
        if let Some(facts) = seat_facts(task, session, &events, &seen) {
            if seen.confirmed_by_request {
                confirm(&live, &facts, &events).await;
            }
            seats.push(facts);
        }
    }
    run(&live, &SystemClock, &seats, &[], &[]).await
}
