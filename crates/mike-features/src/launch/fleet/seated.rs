use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::events::FleetEvent;
use super::fold::{Folded, fold};
use super::plan::worker_seat_name;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::UnsettledFleetLaunch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    pub name: String,
    pub task: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seated {
    live: BTreeMap<Uuid, Seat>,
}

impl Seated {
    pub fn seat(&self, session: Uuid) -> Option<&Seat> {
        self.live.get(&session)
    }

    pub fn task_of(&self, session: Uuid) -> Option<&str> {
        self.seat(session)?.task.as_deref()
    }

    pub fn holds(&self, session: Uuid, task: &str) -> bool {
        self.task_of(session) == Some(task)
    }

    pub fn acceptance_seated(&self, def: &FleetDefinition) -> bool {
        def.acceptance
            .session
            .as_deref()
            .and_then(|session| session.trim().parse::<Uuid>().ok())
            .is_some_and(|session| self.seat(session).is_some())
    }

    pub fn control_seated(&self, def: &FleetDefinition) -> bool {
        [&def.coordinator, &def.acceptance]
            .into_iter()
            .filter_map(|seat| seat.session.as_deref()?.trim().parse::<Uuid>().ok())
            .any(|session| self.seat(session).is_some())
    }

    pub fn worker(mut self, task: &str, session: Uuid, name: &str) -> Self {
        self.live.insert(
            session,
            Seat {
                name: name.to_string(),
                task: Some(task.to_string()),
            },
        );
        self
    }

    pub fn with_everything(mut self, session: Uuid, name: &str) -> Self {
        self.live.insert(
            session,
            Seat {
                name: name.to_string(),
                task: None,
            },
        );
        self
    }

    pub fn from_record(
        def: &FleetDefinition,
        events: &[FleetEvent],
        launches: &[UnsettledFleetLaunch],
    ) -> Self {
        let folded = fold(events, DateTime::<Utc>::MAX_UTC);
        let mut live = recorded(def, &folded, launches);
        for launch in launches
            .iter()
            .filter(|l| l.opened && l.closed_at.is_none())
        {
            let (Some(session), Some(name)) = (launch.session_uuid, launch.seat.as_deref()) else {
                continue;
            };
            if live.contains_key(&session) || folded.seat_over(session) {
                continue;
            }
            live.insert(
                session,
                Seat {
                    name: name.to_string(),
                    task: task_named(def, name),
                },
            );
        }
        Self { live }
    }

    pub async fn of_run(persist: &PersistHandle, run: &str, def: &FleetDefinition) -> Self {
        let events = persist
            .fleet()
            .fleet_events_since(run, 0)
            .await
            .unwrap_or_default();
        let launches = persist
            .fleet()
            .unsettled_fleet_launches(run)
            .await
            .unwrap_or_default();
        Self::from_record(def, &events, &launches)
    }
}

fn recorded(
    def: &FleetDefinition,
    folded: &Folded,
    launches: &[UnsettledFleetLaunch],
) -> BTreeMap<Uuid, Seat> {
    let named: BTreeMap<Uuid, &str> = launches
        .iter()
        .filter_map(|l| Some((l.session_uuid?, l.seat.as_deref()?)))
        .collect();
    folded
        .live_seats()
        .map(|(session, task)| {
            let task = task.filter(|t| !t.is_empty()).map(str::to_string);
            let name = named
                .get(&session)
                .map(|name| (*name).to_string())
                .or_else(|| derived_name(def, task.as_deref()?))
                .unwrap_or_default();
            (session, Seat { name, task })
        })
        .collect()
}

fn derived_name(def: &FleetDefinition, task: &str) -> Option<String> {
    Some(worker_seat_name(&def.tasks.get(task)?.discipline, task))
}

pub fn task_named(def: &FleetDefinition, seat: &str) -> Option<String> {
    def.tasks.iter().find_map(|(id, task)| {
        let base = worker_seat_name(&task.discipline, id);
        let successor = seat
            .strip_prefix(base.as_str())
            .is_some_and(|rest| rest.starts_with('-'));
        (seat == base || successor).then(|| id.clone())
    })
}
