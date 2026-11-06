use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::events::{EventPayload, SeatClosed, SeatLaunchFailed, SeatProcessGone, SeatWoken};

#[path = "supervise_facts.rs"]
mod facts;
#[path = "supervise_live.rs"]
mod live;
#[cfg(test)]
#[path = "supervise_tests.rs"]
mod supervise_tests;

pub use live::pass;

pub const LAUNCH_DEADLINE: Duration = Duration::from_secs(90);
pub const SILENT_AFTER: Duration = Duration::from_secs(300);
pub const WAKE_GRACE: Duration = Duration::from_secs(120);
pub const RELAUNCHES: u32 = 2;
pub const CHECK_REQUEUES: u32 = 1;
pub const STOP_KILLS: u32 = 3;

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatFacts {
    pub task: String,
    pub session: Uuid,
    pub launch_id: i64,
    pub requested_at: DateTime<Utc>,
    pub confirmed: bool,
    pub closed: bool,
    pub process_gone: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub unread_mail: bool,
    pub woken_at: Option<DateTime<Utc>>,
    pub relaunches: u32,
    pub route_suspect: bool,
    pub stopping: bool,
    pub stop_kills: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFacts {
    pub receipt_id: i64,
    pub task: Option<String>,
    pub started_at: DateTime<Utc>,
    pub timeout: Duration,
    pub requeues: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFacts {
    pub slot: String,
    pub dirty: bool,
    pub suspect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    RecordLaunchFailed {
        task: String,
        session: Uuid,
        launch_id: i64,
        reason: String,
    },
    KillSeat {
        task: String,
        session: Uuid,
    },
    Relaunch {
        task: String,
        session: Uuid,
        attempt: u32,
    },
    RouteSuspect {
        task: String,
        session: Uuid,
        words: String,
    },
    StopKill {
        task: String,
        session: Uuid,
    },
    Wake {
        task: String,
        session: Uuid,
    },
    RecordProcessGone {
        task: String,
        session: Uuid,
        launch_id: i64,
    },
    Replace {
        task: String,
        session: Uuid,
        reason: String,
    },
    RecordUnkillable {
        task: String,
        session: Uuid,
    },
    KillCheck {
        receipt_id: i64,
    },
    RequeueCheck {
        receipt_id: i64,
    },
    FailCheck {
        receipt_id: i64,
        task: Option<String>,
    },
    SlotSuspect {
        slot: String,
    },
    NeedsYou {
        task: Option<String>,
        words: String,
    },
}

#[async_trait::async_trait]
pub trait Effects: Send + Sync {
    async fn apply(&self, action: &Action) -> Result<(), String>;
}

fn seconds(since: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - since).num_seconds()
}

fn passed(since: DateTime<Utc>, now: DateTime<Utc>, limit: Duration) -> bool {
    seconds(since, now) >= i64::try_from(limit.as_secs()).unwrap_or(i64::MAX)
}

fn unconfirmed(seat: &SeatFacts, now: DateTime<Utc>) -> Vec<Action> {
    let reason = format!(
        "no first request within {} seconds of the launch request",
        LAUNCH_DEADLINE.as_secs()
    );
    let mut steps = vec![
        Action::RecordLaunchFailed {
            task: seat.task.clone(),
            session: seat.session,
            launch_id: seat.launch_id,
            reason: reason.clone(),
        },
        Action::KillSeat {
            task: seat.task.clone(),
            session: seat.session,
        },
    ];
    if seat.relaunches < RELAUNCHES {
        steps.push(Action::Relaunch {
            task: seat.task.clone(),
            session: seat.session,
            attempt: seat.relaunches + 1,
        });
        return steps;
    }
    let words = format!(
        "task {}: the launch route failed {} times; the seat never made a first request ({reason}). Seen at {}. The daemon's words are in the launch row {}.",
        seat.task,
        seat.relaunches + 1,
        now.to_rfc3339(),
        seat.launch_id
    );
    steps.push(Action::RouteSuspect {
        task: seat.task.clone(),
        session: seat.session,
        words,
    });
    steps
}

fn silent(seat: &SeatFacts, now: DateTime<Utc>) -> Vec<Action> {
    let Some(seen) = seat.last_seen else {
        return Vec::new();
    };
    if !seat.unread_mail || !passed(seen, now, SILENT_AFTER) {
        return Vec::new();
    }
    match seat.woken_at {
        None => vec![Action::Wake {
            task: seat.task.clone(),
            session: seat.session,
        }],
        Some(woken) if woken > seen && passed(woken, now, WAKE_GRACE) => vec![
            Action::KillSeat {
                task: seat.task.clone(),
                session: seat.session,
            },
            Action::Replace {
                task: seat.task.clone(),
                session: seat.session,
                reason: "silent with unread mail after a wake".to_string(),
            },
        ],
        Some(woken) if woken <= seen => vec![Action::Wake {
            task: seat.task.clone(),
            session: seat.session,
        }],
        Some(_) => Vec::new(),
    }
}

fn stopping(seat: &SeatFacts) -> Vec<Action> {
    if seat.stop_kills < STOP_KILLS {
        return vec![Action::StopKill {
            task: seat.task.clone(),
            session: seat.session,
        }];
    }
    vec![
        Action::RecordUnkillable {
            task: seat.task.clone(),
            session: seat.session,
        },
        Action::NeedsYou {
            task: Some(seat.task.clone()),
            words: format!(
                "task {}: the seat survived {} kills at a stop and is recorded unkillable",
                seat.task, STOP_KILLS
            ),
        },
    ]
}

pub fn decide_seat(seat: &SeatFacts, now: DateTime<Utc>) -> Vec<Action> {
    if seat.closed || seat.route_suspect {
        return Vec::new();
    }
    if seat.stopping {
        if seat.process_gone {
            return Vec::new();
        }
        return stopping(seat);
    }
    if seat.process_gone {
        return vec![
            Action::RecordProcessGone {
                task: seat.task.clone(),
                session: seat.session,
                launch_id: seat.launch_id,
            },
            Action::Replace {
                task: seat.task.clone(),
                session: seat.session,
                reason: "the seat's process is gone".to_string(),
            },
        ];
    }
    if !seat.confirmed {
        if passed(seat.requested_at, now, LAUNCH_DEADLINE) {
            return unconfirmed(seat, now);
        }
        return Vec::new();
    }
    silent(seat, now)
}

pub fn decide_check(check: &CheckFacts, now: DateTime<Utc>) -> Vec<Action> {
    if !passed(check.started_at, now, check.timeout) {
        return Vec::new();
    }
    let mut steps = vec![Action::KillCheck {
        receipt_id: check.receipt_id,
    }];
    if check.requeues < CHECK_REQUEUES {
        steps.push(Action::RequeueCheck {
            receipt_id: check.receipt_id,
        });
    } else {
        steps.push(Action::FailCheck {
            receipt_id: check.receipt_id,
            task: check.task.clone(),
        });
    }
    steps
}

pub fn decide_slots(slots: &[SlotFacts]) -> Vec<Action> {
    let mut steps: Vec<Action> = slots
        .iter()
        .filter(|slot| slot.dirty && !slot.suspect)
        .map(|slot| Action::SlotSuspect {
            slot: slot.slot.clone(),
        })
        .collect();
    let every = !slots.is_empty() && slots.iter().all(|slot| slot.suspect || slot.dirty);
    if every && !steps.is_empty() {
        steps.push(Action::NeedsYou {
            task: None,
            words: format!(
                "every build slot ({}) is dirty or unexpected and recorded suspect",
                slots.len()
            ),
        });
    }
    steps
}

pub fn payload(action: &Action) -> Option<EventPayload> {
    match action {
        Action::RecordLaunchFailed {
            launch_id, reason, ..
        } => Some(EventPayload::SeatLaunchFailed(SeatLaunchFailed {
            launch_id: *launch_id,
            reason: reason.clone(),
        })),
        Action::RecordProcessGone { launch_id, .. } => {
            Some(EventPayload::SeatProcessGone(SeatProcessGone {
                launch_id: *launch_id,
            }))
        }
        Action::Wake { .. } => Some(EventPayload::SeatWoken(SeatWoken {
            via: "remote_control".to_string(),
        })),
        Action::RecordUnkillable { .. } => Some(EventPayload::SeatClosed(SeatClosed {
            reason: "unkillable".to_string(),
        })),
        Action::Replace { reason, .. } => Some(EventPayload::SeatClosed(SeatClosed {
            reason: reason.clone(),
        })),
        _ => None,
    }
}

pub async fn run<E: Effects>(
    effects: &E,
    clock: &impl Clock,
    seats: &[SeatFacts],
    checks: &[CheckFacts],
    slots: &[SlotFacts],
) -> Vec<Action> {
    let now = clock.now();
    let wanted: Vec<Action> = seats
        .iter()
        .flat_map(|seat| decide_seat(seat, now))
        .chain(checks.iter().flat_map(|check| decide_check(check, now)))
        .chain(decide_slots(slots))
        .collect();
    let mut done = Vec::new();
    for action in wanted {
        if let Err(why) = effects.apply(&action).await {
            tracing::warn!(?action, %why, "fleet supervisor: an action failed; the next pass decides again");
            continue;
        }
        done.push(action);
    }
    done
}

pub fn finished_tasks(
    status: &crate::launch::fleet::status::FleetStatus,
) -> std::collections::BTreeSet<String> {
    status
        .tasks
        .iter()
        .filter(|task| matches!(task.state.as_str(), "passed" | "awaiting acceptance"))
        .map(|task| task.id.clone())
        .collect()
}

pub fn timed_out(result: &(Option<i32>, String)) -> bool {
    result.0.is_none() && result.1.contains("was stopped after")
}

pub fn needs_you_title(task: Option<&str>) -> String {
    match task {
        Some(task) => format!("NEEDS YOU: task {task} stopped recovering by itself"),
        None => "NEEDS YOU: the fleet stopped recovering by itself".to_string(),
    }
}
