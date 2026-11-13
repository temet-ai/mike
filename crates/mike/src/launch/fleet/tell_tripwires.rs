use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::status::FleetStatus;
use super::tell::Notice;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::ClosedMailAttempt;

pub(super) fn closed_with_work(status: &FleetStatus, coordinator: Uuid) -> Vec<Notice> {
    status.tasks.iter()
        .filter(|task| !task.ready && !matches!(task.state.as_str(), "done" | "passed"))
        .filter(|task| task.seat.is_none())
        .filter_map(|task| {
            let seat = status.seats.iter().find(|seat| Some(&seat.seat) == task.last_seat.as_ref() && seat.closed)?;
            Some(Notice {
                to: coordinator,
                key: format!("closed-work:{}:{:?}", task.id, seat.session),
                covers: Vec::new(),
                body: format!("[mike tripwire] {} (session {:?}) is closed with task {} still {} and not ready. Check its handoff and arrange a successor if work remains; the closed session cannot receive new work.", seat.seat, seat.session, task.id, task.state),
            })
        }).collect()
}

fn lookback(now: DateTime<Utc>, started_at: Option<DateTime<Utc>>) -> DateTime<Utc> {
    let horizon = now - Duration::hours(24);
    started_at.map_or(horizon, |start| start.max(horizon))
}

fn closed_mail(attempt: ClosedMailAttempt, coordinator: Uuid) -> Notice {
    Notice {
        to: coordinator,
        key: format!("closed-mail:{}", attempt.id),
        covers: Vec::new(),
        body: format!(
            "[mike tripwire] Session {} attempted to mail closed seat {} (session {}) at {}. The delivery was refused. Review the sender's routing and arrange a successor if work remains; no message body is included.",
            attempt.sender,
            attempt.seat.as_deref().unwrap_or("unnamed"),
            attempt.recipient,
            attempt.attempted_at.to_rfc3339(),
        ),
    }
}

pub(super) async fn read(
    persist: &PersistHandle,
    run: &str,
    status: &FleetStatus,
    coordinator: Uuid,
) -> Vec<Notice> {
    match persist
        .fleet()
        .fleet_closed_mail_attempts(run, lookback(status.now, status.started_at), status.now)
        .await
    {
        Ok(attempts) => attempts
            .into_iter()
            .map(|attempt| closed_mail(attempt, coordinator))
            .collect(),
        Err(err) => {
            tracing::warn!(%run, error = %err, "fleet: could not read closed-seat delivery refusals");
            Vec::new()
        }
    }
}

#[cfg(test)]
#[path = "tell_tripwires_tests.rs"]
mod tests;
