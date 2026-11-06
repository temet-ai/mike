use std::hash::{DefaultHasher, Hash, Hasher};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::ControlSeats;
use super::report::Waiting;
use super::reuse::{slots, waiting_uses};
use super::status::FleetStatus;
use super::status_types::AcceptanceObligation;
use super::tell::Notice;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeResultRow;

pub const IDLE_SHOWN_SECONDS: i64 = 300;
const DIGEST_EVERY_SECONDS: i64 = 180;
const WAITING_ALERT_MINUTES: i64 = 10;
const QUEUE_WINDOW_HOURS: i64 = 6;
const QUEUE_ROWS_READ: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DigestRow {
    pub task: String,
    pub seat: Option<String>,
    pub state: String,
    pub ready: bool,
    pub accepted: bool,
    pub idle_seconds: Option<i64>,
    pub last_request_at: Option<DateTime<Utc>>,
    pub passes: usize,
    pub fails: usize,
}

impl DigestRow {
    pub fn idle_since(&self) -> Option<DateTime<Utc>> {
        self.idle_seconds
            .filter(|s| *s >= IDLE_SHOWN_SECONDS)
            .and(self.last_request_at)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlotQueue {
    pub waiting: usize,
    pub slots: usize,
}

pub fn queue_of(def: &FleetDefinition, rows: &[TreeResultRow]) -> SlotQueue {
    if !def.execution.cargo {
        return SlotQueue::default();
    }
    SlotQueue {
        waiting: waiting_uses(rows, &def.wrappers, slots(def)).len(),
        slots: slots(def),
    }
}

pub async fn slot_queue(persist: &PersistHandle, def: &FleetDefinition, run: &str) -> SlotQueue {
    let since = Utc::now() - Duration::hours(QUEUE_WINDOW_HOURS);
    match persist
        .tree_results(run, None, since, QUEUE_ROWS_READ)
        .await
    {
        Ok(rows) => queue_of(def, &rows),
        Err(err) => {
            tracing::warn!(%run, %err, "fleet: could not read the seats' commands for the slot queue");
            SlotQueue::default()
        }
    }
}

pub fn rows(status: &FleetStatus) -> Vec<DigestRow> {
    status
        .tasks
        .iter()
        .map(|task| {
            let last = task
                .seat
                .as_deref()
                .and_then(|name| {
                    status
                        .seats
                        .iter()
                        .find(|seat| seat.seat == name && !seat.closed)
                })
                .and_then(|seat| seat.last_request_at);
            DigestRow {
                task: task.id.clone(),
                seat: task.seat.clone(),
                state: task.state.clone(),
                ready: task.ready,
                accepted: !task.criteria.is_empty()
                    && task.passed_criteria.len() == task.criteria.len(),
                idle_seconds: last.map(|at| (status.now - at).num_seconds().max(0)),
                last_request_at: last,
                passes: task.passes,
                fails: task.fails,
            }
        })
        .collect()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn clock(at: DateTime<Utc>) -> String {
    at.format("%H:%M:%SZ").to_string()
}

pub fn task_line(row: &DigestRow) -> String {
    let idle = row
        .idle_since()
        .map(|at| format!(", idle since {}", clock(at)))
        .unwrap_or_default();
    format!(
        "- {} ({}): {}{idle}, ready {}, accepted {}, {}, {}",
        row.task,
        row.seat.as_deref().unwrap_or("no seat"),
        row.state,
        row.ready,
        row.accepted,
        plural(row.passes, "pass", "passes"),
        plural(row.fails, "fail", "fails")
    )
}

pub fn reports_line(now: DateTime<Utc>, waiting: &[Waiting]) -> Option<String> {
    if waiting.is_empty() {
        return None;
    }
    let each: Vec<String> = waiting
        .iter()
        .map(|w| {
            format!(
                "{} ({}) {} min, {}",
                w.task,
                if w.seat.is_empty() {
                    "no seat"
                } else {
                    &w.seat
                },
                (now - w.since).num_minutes().max(0),
                plural(w.reports, "report", "reports")
            )
        })
        .collect();
    Some(format!(
        "Reports waiting for a verdict, oldest first: {}.",
        each.join("; ")
    ))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Waits<'a> {
    pub reports: &'a [Waiting],
    pub questions: &'a [(Uuid, String)],
}

struct Change {
    key: String,
    line: String,
}

fn hashed(value: impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn obligation_line(obligation: &AcceptanceObligation) -> String {
    format!(
        "Obligation {}: {} — {}{}",
        obligation.obligation_id,
        obligation.outcome,
        obligation.title,
        obligation
            .issue
            .as_ref()
            .map(|issue| format!(" ({issue})"))
            .unwrap_or_default()
    )
}

fn row_changes(role: &str, rows: &[DigestRow]) -> Vec<Change> {
    rows.iter()
        .map(|row| Change {
            key: format!(
                "digest-row:{role}:{}:{}",
                row.task,
                hashed((
                    &row.seat,
                    &row.state,
                    row.ready,
                    row.passes,
                    row.fails,
                    row.accepted
                ))
            ),
            line: task_line(row),
        })
        .collect()
}

fn register_changes(role: &str, status: &FleetStatus) -> Vec<Change> {
    let fleet = Change {
        key: format!("digest-fleet:{role}:{}", status.accepted),
        line: format!("Fleet accepted: {}.", status.accepted),
    };
    let obligations = status
        .obligations
        .iter()
        .filter(|obligation| obligation.required)
        .map(|obligation| Change {
            key: format!(
                "digest-obligation:{role}:{}",
                hashed((
                    obligation.note_id,
                    obligation.updated_at,
                    &obligation.outcome,
                    &obligation.issue
                ))
            ),
            line: obligation_line(obligation),
        });
    std::iter::once(fleet).chain(obligations).collect()
}

fn wait_changes(role: &str, now: DateTime<Utc>, waits: Waits) -> Vec<Change> {
    let reports = waits
        .reports
        .iter()
        .filter(|w| now - w.since >= Duration::minutes(WAITING_ALERT_MINUTES))
        .filter_map(|w| {
            Some(Change {
                key: format!("digest-wait:{role}:{}:{}", w.task, w.newest),
                line: reports_line(now, std::slice::from_ref(w))?,
            })
        });
    let questions = waits.questions.iter().map(|(id, line)| Change {
        key: format!("digest-question:{role}:{id}"),
        line: format!("Blocked question open five minutes or more: {line}"),
    });
    reports.chain(questions).collect()
}

fn changes(
    role: &str,
    (status, rows, waits): (&FleetStatus, &[DigestRow], Waits),
    said: &impl Fn(&str) -> bool,
) -> Vec<Change> {
    row_changes(role, rows)
        .into_iter()
        .chain(register_changes(role, status))
        .chain(wait_changes(role, status.now, waits))
        .filter(|change| !said(&change.key))
        .collect()
}

fn delta_text(status: &FleetStatus, changed: &[Change], queue: SlotQueue) -> String {
    let mut lines = vec![format!(
        "[mike digest] Fleet {} at {}. Only what changed since the last digest; get_fleet view=digest has the whole table.",
        status.slug,
        clock(status.now)
    )];
    lines.extend(changed.iter().map(|change| change.line.clone()));
    if queue.waiting > 0 {
        lines.push(format!(
            "Builds waiting for a slot: {} ({}).",
            queue.waiting,
            plural(queue.slots, "slot", "slots")
        ));
    }
    if let Some(quiet) = status.every_seat_idle_seconds {
        lines.push(format!("Every seat idle for {} min.", quiet / 60));
    }
    lines.join("\n")
}

fn recipients(seats: &ControlSeats) -> impl Iterator<Item = (&'static str, Uuid)> {
    [
        ("coordinator", Some(seats.coordinator)),
        ("acceptance", Some(seats.acceptance)),
        ("observer", seats.observer),
    ]
    .into_iter()
    .filter_map(|(role, session)| Some((role, session?)))
}

pub fn digests(
    status: &FleetStatus,
    seats: &ControlSeats,
    (queue, waits): (SlotQueue, Waits),
    said: impl Fn(&str) -> bool,
) -> Vec<Notice> {
    let rows = rows(status);
    let window = status.now.timestamp().div_euclid(DIGEST_EVERY_SECONDS);
    recipients(seats)
        .filter_map(|(role, to)| {
            let recent = format!("digest-at:{role}:{window}");
            if said(&recent) {
                return None;
            }
            let changed = changes(role, (status, &rows, waits), &said);
            if changed.is_empty() {
                return None;
            }
            let keys: Vec<String> = changed.iter().map(|change| change.key.clone()).collect();
            let key = format!("digest:{role}:{}", hashed(&keys));
            Some(Notice {
                to,
                key,
                covers: keys.into_iter().chain(std::iter::once(recent)).collect(),
                body: delta_text(status, &changed, queue),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "tell_digest_tests.rs"]
mod tests;
