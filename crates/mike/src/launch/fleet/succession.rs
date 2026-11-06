use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::{ControlSeats, FleetFacts, REFERENCE_SEAT};
use super::status::{FleetStatus, SeatStatus};
use super::tell_context::REPLACE_AT;
use super::told;
use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub seat: String,
    pub session: Uuid,
    pub context_tokens: i64,
}

fn is_reference(seat: &SeatStatus) -> bool {
    seat.role == REFERENCE_SEAT || seat.seat == REFERENCE_SEAT
}

pub fn key(session: Uuid) -> String {
    format!("reference-successor:{session}")
}

pub fn due(status: &FleetStatus, said: impl Fn(&str) -> bool) -> Vec<Due> {
    status
        .seats
        .iter()
        .filter(|s| !s.closed && is_reference(s))
        .filter_map(|s| {
            let session = s.session?;
            let tokens = s.context_tokens.filter(|t| *t >= REPLACE_AT)?;
            (!said(&key(session))).then(|| Due {
                seat: s.seat.clone(),
                session,
                context_tokens: tokens,
            })
        })
        .collect()
}

pub async fn pass(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    status: &FleetStatus,
) {
    let Some(run_id) = row.run_id.as_deref() else {
        return;
    };
    if status.state != "running" {
        return;
    }
    let told_already = told::load(persist, run_id).await;
    let due = due(status, |k| told_already.already(k));
    if due.is_empty() {
        return;
    }
    let facts = facts_for(row, def, run_id);
    for old in due {
        told::mark(persist, run_id, &[key(old.session)]).await;
        let new = super::warm::succeed_reference(state, persist, def, &facts, old.session).await;
        log_succession(run_id, &old, new);
    }
}

fn facts_for(row: &FleetRow, def: &FleetDefinition, run_id: &str) -> FleetFacts {
    FleetFacts {
        fleet: row.fleet_uuid.to_string(),
        slug: row.slug.clone(),
        run_id: run_id.to_string(),
        seats: ControlSeats::recorded(def).unwrap_or_else(ControlSeats::fresh),
        paths: super::bundle::paths_for(&def.repo),
    }
}

fn log_succession(run_id: &str, old: &Due, new: Option<Uuid>) {
    match new {
        Some(new) => log_successor_started(run_id, old, new),
        None => log_successor_missing(run_id, old),
    }
}

fn log_successor_started(run_id: &str, old: &Due, new: Uuid) {
    tracing::info!(
        run = run_id, old = %old.session, %new, tokens = old.context_tokens,
        "fleet: the reference seat passed the context line; mike started its successor and closed it"
    );
}

fn log_successor_missing(run_id: &str, old: &Due) {
    tracing::warn!(
        run = run_id, old = %old.session, tokens = old.context_tokens,
        "fleet: the reference seat passed the context line but no successor could be started; it stays open"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    use super::{REPLACE_AT, due, key};
    use crate::launch::fleet::status::{FleetStatus, SeatStatus};

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap_or_default()
    }

    fn seat(
        name: &str,
        role: &str,
        session: u128,
        tokens: Option<i64>,
        closed: bool,
    ) -> SeatStatus {
        SeatStatus {
            seat: name.into(),
            role: role.into(),
            model: "sonnet".into(),
            state: "idle".into(),
            session: Some(Uuid::from_u128(session)),
            goal: None,
            context_tokens: tokens,
            requests: 3,
            last_request_at: Some(at("2026-11-01T09:00:00Z")),
            opened_at: Some(at("2026-11-01T07:00:00Z")),
            closed,
            first_request_cache: None,
        }
    }

    fn status(seats: Vec<SeatStatus>) -> FleetStatus {
        FleetStatus {
            fleet: "f".into(),
            slug: "s".into(),
            state: "running".into(),
            run_id: Some("fleet-s-1".into()),
            goal: String::new(),
            started_at: Some(at("2026-11-01T07:00:00Z")),
            finished_at: None,
            now: at("2026-11-01T09:08:00Z"),
            missing: vec![],
            criteria: vec![],
            tasks: vec![],
            seats,
            verdicts: vec![],
            blocked: vec![],
            needs_operator: vec![],
            accepted: false,
            obligations: vec![],
            every_seat_idle_seconds: None,
            tree_broken: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn only_an_open_reference_seat_past_the_line_not_yet_succeeded_is_due() {
        let seats = vec![
            seat("reference", "reference", 1, Some(REPLACE_AT), false),
            seat("reference-2", "reference", 2, Some(REPLACE_AT + 5), true),
            seat("ts-T1", "ts", 3, Some(900_000), false),
            seat("reference", "reference", 4, Some(REPLACE_AT - 1), false),
            seat("reference", "reference", 5, None, false),
        ];
        let found = due(&status(seats), |_| false);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].session, Uuid::from_u128(1));
        assert_eq!(found[0].context_tokens, REPLACE_AT);
    }

    #[test]
    fn a_reference_seat_already_given_a_successor_is_not_due_again() {
        let told: BTreeSet<String> = [key(Uuid::from_u128(1))].into();
        let seats = vec![seat(
            "reference",
            "reference",
            1,
            Some(REPLACE_AT + 1),
            false,
        )];
        assert!(due(&status(seats), |k| told.contains(k)).is_empty());
    }
}
