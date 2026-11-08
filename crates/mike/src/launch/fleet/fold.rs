use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::events::{EventKind, FleetEvent};
use super::status::SeatFacts;

#[derive(Debug, Default, Clone)]
struct SeatLife {
    task: Option<String>,
    confirmed_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    failed_launch: bool,
}

impl SeatLife {
    fn live(&self) -> bool {
        self.confirmed_at.is_some() && self.ended_at.is_none() && !self.failed_launch
    }
}

#[derive(Debug, Default, Clone)]
struct TaskLife {
    sessions: Vec<Uuid>,
    reports: usize,
    reported_at: Option<DateTime<Utc>>,
    verdict_at: Option<DateTime<Utc>>,
    commits: Vec<String>,
    refusals: usize,
    last_refusal: String,
    verdicts: BTreeMap<String, bool>,
}

pub(super) const START_ATTEMPTS: usize = 3;

#[derive(Debug, Default, Clone)]
pub(super) struct Folded {
    seats: BTreeMap<Uuid, SeatLife>,
    tasks: BTreeMap<String, TaskLife>,
    passed_shas: BTreeSet<String>,
    stop_requested: bool,
}

pub(super) fn fold(events: &[FleetEvent], until: DateTime<Utc>) -> Folded {
    let mut ordered: Vec<&FleetEvent> = events.iter().filter(|e| e.occurred_at <= until).collect();
    ordered.sort_by_key(|event| (event.occurred_at, event.id));
    let mut folded = Folded::default();
    for event in ordered {
        folded.apply(event);
    }
    folded
}

impl Folded {
    fn apply(&mut self, event: &FleetEvent) {
        if let Some(session) = event.session_uuid {
            let seat = self.seats.entry(session).or_default();
            if seat.task.is_none() {
                seat.task = event.task.clone();
            }
        }
        match event.kind {
            EventKind::SeatLaunchConfirmed => self.launch_confirmed(event),
            EventKind::SeatLaunchFailed => self.launch_failed(event),
            EventKind::SeatClosed | EventKind::SeatProcessGone => self.seat_ended(event),
            EventKind::CommitLanded => self.commit_landed(event),
            EventKind::CheckFinished => self.check_finished(event),
            EventKind::ReportFiled => self.report_filed(event),
            EventKind::VerdictRecorded => self.verdict_recorded(event),
            EventKind::StopRequested => self.stop_requested = true,
            _ => {}
        }
    }

    fn task_entry(&mut self, event: &FleetEvent) -> &mut TaskLife {
        self.tasks
            .entry(event.task.clone().unwrap_or_default())
            .or_default()
    }

    fn launch_confirmed(&mut self, event: &FleetEvent) {
        let Some(session) = event.session_uuid else {
            return;
        };
        let seat = self.seats.entry(session).or_default();
        seat.confirmed_at = Some(event.occurred_at);
        seat.ended_at = None;
        seat.failed_launch = false;
        seat.task = event.task.clone();
        if let Some(task) = &event.task {
            let life = self.tasks.entry(task.clone()).or_default();
            life.sessions.push(session);
            life.refusals = 0;
        }
    }

    fn launch_failed(&mut self, event: &FleetEvent) {
        if let Some(session) = event.session_uuid {
            self.seats.entry(session).or_default().failed_launch = true;
        }
        if event.task.is_some() {
            let reason = event.payload.get("reason").and_then(Value::as_str);
            let life = self.task_entry(event);
            life.refusals += 1;
            life.last_refusal = reason.unwrap_or("the launch was refused").to_string();
        }
    }

    pub(super) fn not_started(&self, task: &str) -> Option<String> {
        let life = self.tasks.get(task)?;
        (life.refusals >= START_ATTEMPTS && self.seat_of(task).is_none()).then(|| {
            format!(
                "not started after {} attempts: {}; set the task's seat and session to null to try again",
                life.refusals, life.last_refusal
            )
        })
    }

    fn report_filed(&mut self, event: &FleetEvent) {
        let life = self.task_entry(event);
        life.reports += 1;
        life.reported_at = Some(event.occurred_at);
    }

    pub(super) fn rounds(&self, task: &str) -> usize {
        self.tasks.get(task).map_or(0, |life| life.reports)
    }

    fn seat_ended(&mut self, event: &FleetEvent) {
        if let Some(session) = event.session_uuid {
            self.seats.entry(session).or_default().ended_at = Some(event.occurred_at);
        }
    }

    fn commit_landed(&mut self, event: &FleetEvent) {
        let Some(sha) = event.payload.get("sha").and_then(Value::as_str) else {
            return;
        };
        if event.task.is_some() {
            self.task_entry(event).commits.push(sha.to_string());
        }
    }

    fn check_finished(&mut self, event: &FleetEvent) {
        let passed = event.payload.get("passed").and_then(Value::as_bool) == Some(true);
        let sha = event.payload.get("sha").and_then(Value::as_str);
        if let (true, Some(sha)) = (passed, sha) {
            self.passed_shas.insert(sha.to_string());
        }
    }

    pub(super) fn seat_of(&self, task: &str) -> Option<Uuid> {
        self.tasks
            .get(task)?
            .sessions
            .iter()
            .rev()
            .find(|session| self.seats.get(session).is_some_and(SeatLife::live))
            .copied()
    }

    pub(super) fn last_seat(&self, task: &str) -> Option<Uuid> {
        self.tasks.get(task)?.sessions.last().copied()
    }

    pub(super) fn task_of(&self, session: Uuid) -> Option<&str> {
        self.seats
            .get(&session)?
            .task
            .as_deref()
            .filter(|t| !t.is_empty())
    }

    pub(super) fn live_seats(&self) -> impl Iterator<Item = (Uuid, Option<&str>)> {
        self.seats
            .iter()
            .filter(|(_, life)| life.live())
            .map(|(session, life)| (*session, life.task.as_deref()))
    }

    pub(super) fn seat_over(&self, session: Uuid) -> bool {
        self.seats
            .get(&session)
            .is_some_and(|life| life.ended_at.is_some() || life.failed_launch)
    }

    fn verdict_recorded(&mut self, event: &FleetEvent) {
        let criterion = event.payload.get("criterion").and_then(Value::as_str);
        let passed = event.payload.get("passed").and_then(Value::as_bool);
        let at = event.occurred_at;
        let life = self.task_entry(event);
        life.verdict_at = Some(at);
        if let (Some(criterion), Some(passed)) = (criterion, passed) {
            life.verdicts.insert(criterion.to_string(), passed);
        }
    }

    pub(super) fn done_tasks(&self, def: &FleetDefinition) -> BTreeSet<String> {
        def.tasks
            .iter()
            .filter(|(id, task)| {
                !task.criteria.is_empty()
                    && !self.awaiting_verdict(id)
                    && self.tasks.get(*id).is_some_and(|life| {
                        task.criteria
                            .iter()
                            .all(|criterion| life.verdicts.get(criterion) == Some(&true))
                    })
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub(super) fn awaiting_verdict(&self, task: &str) -> bool {
        self.tasks
            .get(task)
            .is_some_and(|life| match (life.reported_at, life.verdict_at) {
                (Some(reported), Some(verdict)) => reported > verdict,
                (Some(_), None) => true,
                _ => false,
            })
    }

    pub(super) fn exports_passed(&self, task: &str) -> bool {
        self.tasks.get(task).is_some_and(|life| {
            life.commits
                .iter()
                .any(|sha| self.passed_shas.contains(sha))
        })
    }

    pub(super) fn any_seat_live(&self) -> bool {
        self.seats.values().any(SeatLife::live)
    }

    pub(super) fn has_seats(&self) -> bool {
        self.seats.values().any(|seat| seat.confirmed_at.is_some())
    }

    pub(super) fn fleet_state(&self, recorded: &str, every_task_accepted: bool) -> String {
        if recorded != "running" && recorded != "stopping" {
            return recorded.to_string();
        }
        let live = self.any_seat_live();
        match () {
            _ if self.stop_requested && live => "stopping",
            _ if self.stop_requested => "done",
            _ if self.has_seats() && !live && every_task_accepted => "done",
            _ => recorded,
        }
        .to_string()
    }
}

pub async fn done_tasks_of(
    persist: &crate::store::persistence::PersistHandle,
    run: Option<&str>,
    def: &FleetDefinition,
) -> BTreeSet<String> {
    let Some(run) = run else {
        return BTreeSet::new();
    };
    let events = persist.fleet_events_since(run, 0).await.unwrap_or_default();
    fold(&events, DateTime::<Utc>::MAX_UTC).done_tasks(def)
}

pub(super) fn implied_seat_events(
    run_id: &str,
    def: &FleetDefinition,
    seats: &[SeatFacts],
    recorded: &[FleetEvent],
) -> Vec<FleetEvent> {
    let known: BTreeSet<Uuid> = recorded
        .iter()
        .filter(|e| e.kind == EventKind::SeatLaunchConfirmed)
        .filter_map(|e| e.session_uuid)
        .collect();
    let task_of: BTreeMap<String, String> = def
        .tasks
        .iter()
        .map(|(id, task)| (format!("{}-{id}", task.discipline), id.clone()))
        .collect();
    let mut out = Vec::new();
    for facts in seats {
        let (Some(session), Some(name), Some(opened)) =
            (facts.session, &facts.seat, facts.opened_at)
        else {
            continue;
        };
        if known.contains(&session) {
            continue;
        }
        let task = task_of.get(name).map(String::as_str);
        out.push(implied(
            run_id,
            task,
            session,
            EventKind::SeatLaunchConfirmed,
            opened,
        ));
        if let Some(closed) = facts.closed_at {
            out.push(implied(
                run_id,
                task,
                session,
                EventKind::SeatClosed,
                closed,
            ));
        }
    }
    out
}

fn implied(
    run_id: &str,
    task: Option<&str>,
    session: Uuid,
    kind: EventKind,
    at: DateTime<Utc>,
) -> FleetEvent {
    FleetEvent {
        id: -1,
        run_id: run_id.to_string(),
        task: task.map(str::to_string),
        session_uuid: Some(session),
        kind,
        payload: json!({}),
        occurred_at: at,
    }
}
