use std::sync::LazyLock;

use dashmap::DashMap;
use uuid::Uuid;

use super::fleet::plan::TEMPLATE_SEAT_PREFIX;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const CACHED_SESSIONS_CAP: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeatRole {
    Plain,
    Template,
    Fork,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SeatFact {
    pub(crate) fleet_seat: bool,
    pub(crate) role: SeatRole,
}

impl SeatFact {
    pub(crate) const OUTSIDE_ANY_FLEET: SeatFact = SeatFact {
        fleet_seat: false,
        role: SeatRole::Plain,
    };

    pub(crate) fn from_launch(
        run_id: Option<&str>,
        seat: Option<&str>,
        template_id: Option<i64>,
    ) -> Self {
        Self {
            fleet_seat: run_id.is_some_and(|run| !run.trim().is_empty()),
            role: role_from_launch(seat, template_id),
        }
    }
}

static FACTS: LazyLock<DashMap<String, SeatFact>> = LazyLock::new(DashMap::new);

pub(crate) fn role_from_launch(seat: Option<&str>, template_id: Option<i64>) -> SeatRole {
    if seat.is_some_and(|s| s.starts_with(TEMPLATE_SEAT_PREFIX)) {
        SeatRole::Template
    } else if template_id.is_some() {
        SeatRole::Fork
    } else {
        SeatRole::Plain
    }
}

pub(crate) async fn seat_fact(persist: &PersistHandle, session: Uuid) -> SeatFact {
    let key = session.to_string();
    if let Some(known) = FACTS.get(&key) {
        return *known;
    }
    match persist.find_launch_by_session(session).await {
        Ok(row) => {
            let fact = row.as_ref().map_or(SeatFact::OUTSIDE_ANY_FLEET, |launch| {
                SeatFact::from_launch(
                    launch.run_id.as_deref(),
                    launch.seat.as_deref(),
                    launch.template_id,
                )
            });
            remember(key, fact);
            fact
        }
        Err(err) => {
            tracing::warn!(error = %err, session_id = %session, "could not read the launch row of this session; treating it as a session outside any fleet for this call");
            SeatFact::OUTSIDE_ANY_FLEET
        }
    }
}

pub(crate) async fn is_fleet_seat(persist: &PersistHandle, session: Uuid) -> bool {
    seat_fact(persist, session).await.fleet_seat
}

pub(crate) async fn seat_role(state: &AppState, session_id: &str) -> SeatRole {
    if let Some(known) = FACTS.get(session_id) {
        return known.role;
    }
    let (Some(persist), Ok(session)) = (state.persist(), session_id.parse::<Uuid>()) else {
        return SeatRole::Plain;
    };
    seat_fact(persist, session).await.role
}

fn remember(key: String, fact: SeatFact) {
    if FACTS.len() >= CACHED_SESSIONS_CAP {
        FACTS.clear();
    }
    FACTS.insert(key, fact);
}

#[cfg(test)]
pub(crate) fn remember_for_test(session_id: &str, role: SeatRole) {
    remember(
        session_id.to_owned(),
        SeatFact {
            fleet_seat: role != SeatRole::Plain,
            role,
        },
    );
}

#[cfg(test)]
pub(crate) fn remember_fact_for_test(session: Uuid, fact: SeatFact) {
    remember(session.to_string(), fact);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_whose_seat_begins_template_is_a_template() {
        assert_eq!(
            role_from_launch(Some("template-rust-developer"), None),
            SeatRole::Template
        );
    }

    #[test]
    fn a_launch_with_a_template_id_is_a_fork() {
        assert_eq!(
            role_from_launch(Some("rust-developer-T3"), Some(4)),
            SeatRole::Fork
        );
    }

    #[test]
    fn a_launch_with_neither_is_plain() {
        assert_eq!(role_from_launch(None, None), SeatRole::Plain);
        assert_eq!(role_from_launch(Some("coordinator"), None), SeatRole::Plain);
    }

    #[test]
    fn only_a_launch_with_a_run_is_a_fleet_seat() {
        assert!(SeatFact::from_launch(Some("fleet-x-1"), Some("coordinator"), None).fleet_seat);
        assert!(SeatFact::from_launch(Some("fleet-x-1"), Some("template-db-dev"), None).fleet_seat);
        assert!(SeatFact::from_launch(Some("fleet-x-1"), Some("db-dev-T1"), Some(4)).fleet_seat);
        assert!(!SeatFact::from_launch(None, Some("dawid"), None).fleet_seat);
        assert!(!SeatFact::from_launch(Some("  "), None, None).fleet_seat);
        assert!(!SeatFact::from_launch(None, None, None).fleet_seat);
    }

    #[test]
    fn a_remembered_fact_answers_without_the_ledger() {
        let session = Uuid::new_v4();
        remember_fact_for_test(
            session,
            SeatFact {
                fleet_seat: true,
                role: SeatRole::Fork,
            },
        );
        let known = FACTS
            .get(&session.to_string())
            .map(|fact| *fact)
            .unwrap_or(SeatFact::OUTSIDE_ANY_FLEET);
        assert_eq!(
            known,
            SeatFact {
                fleet_seat: true,
                role: SeatRole::Fork
            }
        );
    }
}
