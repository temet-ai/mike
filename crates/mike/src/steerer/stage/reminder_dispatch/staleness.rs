use crate::config::AppState;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::{PersistError, PersistHandle};

const PUSH_ACK_GRACE: chrono::Duration = chrono::Duration::seconds(10);

pub(in crate::steerer::stage) enum SteererGate {
    Deliver,
    Stale,
    PushInFlight,
    WaitForTurnEnd,
}

pub(in crate::steerer::stage) fn is_advisory(reminder: &SystemReminder) -> bool {
    matches!(
        reminder.source,
        ReminderSource::Steerer { advisory: true, .. }
    )
}

pub(in crate::steerer::stage) fn request_is_mid_turn(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .is_ok_and(|body| crate::extract::request::detect_tool_loop(&body))
}

fn steerer_expiry_reason(
    now: chrono::DateTime<chrono::Utc>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    superseded: bool,
) -> Option<&'static str> {
    if expires_at.is_some_and(|expires_at| now >= expires_at) {
        Some("expired_ttl")
    } else if superseded {
        Some("superseded_by_operator")
    } else {
        None
    }
}

fn guard_push_delivered(
    kind: &str,
    pushed_at: Option<chrono::DateTime<chrono::Utc>>,
    acked_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<&'static str> {
    (kind == "turn_guard" && pushed_at.is_some() && acked_at.is_some())
        .then_some("delivered_by_push")
}

fn push_still_in_flight(
    now: chrono::DateTime<chrono::Utc>,
    pushed_at: Option<chrono::DateTime<chrono::Utc>>,
    acked_at: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    acked_at.is_none() && pushed_at.is_some_and(|pushed_at| now < pushed_at + PUSH_ACK_GRACE)
}

pub(in crate::steerer::stage) async fn steerer_reminder_gate(
    state: &AppState,
    session_id: &str,
    reminder: &SystemReminder,
    mid_turn: bool,
) -> SteererGate {
    let ReminderSource::Steerer {
        kind,
        intervention_uuid,
        advisory,
    } = &reminder.source
    else {
        return SteererGate::Deliver;
    };
    let Some(persist) = &state.persist else {
        return SteererGate::Deliver;
    };

    let Some(uuid) = parse_intervention_uuid(session_id, intervention_uuid) else {
        return SteererGate::Deliver;
    };

    let Some(staleness) =
        fetch_intervention_staleness(persist, session_id, intervention_uuid, uuid).await
    else {
        return SteererGate::Deliver;
    };

    let now = chrono::Utc::now();

    let superseded = staleness.superseded && !*advisory;
    let reason = guard_push_delivered(kind, staleness.pushed_at, staleness.acked_at)
        .or_else(|| steerer_expiry_reason(now, staleness.expires_at, superseded));
    if let Some(reason) = reason {
        expire_stale_intervention(persist, session_id, intervention_uuid, uuid, reason).await;
        log_steerer_reminder_stale(session_id, intervention_uuid, kind, reason);
        return SteererGate::Stale;
    }

    if push_still_in_flight(now, staleness.pushed_at, staleness.acked_at) {
        log_steerer_reminder_push_in_flight(session_id, intervention_uuid, kind);
        return SteererGate::PushInFlight;
    }
    if *advisory && mid_turn {
        return SteererGate::WaitForTurnEnd;
    }

    SteererGate::Deliver
}

fn parse_intervention_uuid(session_id: &str, intervention_uuid: &str) -> Option<uuid::Uuid> {
    match uuid::Uuid::parse_str(intervention_uuid) {
        Ok(uuid) => Some(uuid),
        Err(e) => {
            log_intervention_uuid_unparsable(session_id, intervention_uuid, &e);
            None
        }
    }
}

fn log_intervention_uuid_unparsable(session_id: &str, intervention_uuid: &str, e: &uuid::Error) {
    tracing::warn!(
        session_id = %session_id,
        %intervention_uuid,
        error = %e,
        "steerer intervention_uuid unparsable — delivering fail-open"
    );
}

async fn fetch_intervention_staleness(
    persist: &PersistHandle,
    session_id: &str,
    intervention_uuid: &str,
    uuid: uuid::Uuid,
) -> Option<crate::store::persistence::steering::InterventionStaleness> {
    match persist.intervention_staleness(uuid).await {
        Ok(Some(staleness)) => Some(staleness),
        Ok(None) => None,
        Err(e) => {
            log_staleness_check_failed(session_id, intervention_uuid, &e);
            None
        }
    }
}

fn log_staleness_check_failed(session_id: &str, intervention_uuid: &str, e: &PersistError) {
    tracing::warn!(
        session_id = %session_id,
        %intervention_uuid,
        error = %e,
        "steerer staleness check failed — delivering fail-open"
    );
}

async fn expire_stale_intervention(
    persist: &PersistHandle,
    session_id: &str,
    intervention_uuid: &str,
    uuid: uuid::Uuid,
    reason: &str,
) {
    if let Err(e) = persist.expire_intervention(uuid, reason).await {
        log_expire_intervention_failed(session_id, intervention_uuid, &e);
    }
}

fn log_expire_intervention_failed(session_id: &str, intervention_uuid: &str, e: &PersistError) {
    tracing::warn!(
        session_id = %session_id,
        %intervention_uuid,
        error = %e,
        "steerer intervention expire-stamp failed"
    );
}

fn log_steerer_reminder_stale(session_id: &str, intervention_uuid: &str, kind: &str, reason: &str) {
    tracing::debug!(
        session_id = %session_id,
        %intervention_uuid,
        kind = %kind,
        reason,
        "steerer reminder stale at drain — dropped, not injected"
    );
}

fn log_steerer_reminder_push_in_flight(session_id: &str, intervention_uuid: &str, kind: &str) {
    tracing::debug!(
        session_id = %session_id,
        %intervention_uuid,
        kind = %kind,
        "steerer reminder pushed recently with no ack yet — deferring to a later drain"
    );
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::{guard_push_delivered, push_still_in_flight, steerer_expiry_reason};

    #[test]
    fn guard_wake_pushed_and_acked_is_dropped_at_drain() {
        let now = Utc::now();
        assert_eq!(
            guard_push_delivered("turn_guard", Some(now), Some(now)),
            Some("delivered_by_push")
        );
        assert_eq!(guard_push_delivered("turn_guard", Some(now), None), None);
        assert_eq!(guard_push_delivered("verifier", Some(now), Some(now)), None);
    }

    #[test]
    fn steerer_expiry_reason_past_ttl_expired() {
        let now = Utc::now();
        let expires_at = now - Duration::minutes(1);
        assert_eq!(
            steerer_expiry_reason(now, Some(expires_at), false),
            Some("expired_ttl")
        );
    }

    #[test]
    fn steerer_expiry_reason_superseded_without_ttl_hit() {
        let now = Utc::now();
        let expires_at = now + Duration::minutes(10);
        assert_eq!(
            steerer_expiry_reason(now, Some(expires_at), true),
            Some("superseded_by_operator")
        );
    }

    #[test]
    fn steerer_expiry_reason_ttl_takes_priority_over_supersession() {
        let now = Utc::now();
        let expires_at = now - Duration::minutes(1);
        assert_eq!(
            steerer_expiry_reason(now, Some(expires_at), true),
            Some("expired_ttl")
        );
    }

    #[test]
    fn steerer_expiry_reason_fresh_mid_turn_still_delivers() {
        let now = Utc::now();
        let expires_at = now + Duration::minutes(10);
        assert_eq!(steerer_expiry_reason(now, Some(expires_at), false), None);
    }

    #[test]
    fn steerer_expiry_reason_no_expires_at_never_ttl_expires() {
        let now = Utc::now();
        assert_eq!(steerer_expiry_reason(now, None, false), None);
    }

    #[test]
    fn push_still_in_flight_within_grace_window_and_unacked() {
        let now = Utc::now();
        let pushed_at = now - Duration::seconds(3);
        assert!(push_still_in_flight(now, Some(pushed_at), None));
    }

    #[test]
    fn push_still_in_flight_false_once_grace_window_elapsed() {
        let now = Utc::now();
        let pushed_at = now - Duration::seconds(11);
        assert!(!push_still_in_flight(now, Some(pushed_at), None));
    }

    #[test]
    fn push_still_in_flight_false_once_acked_even_within_window() {
        let now = Utc::now();
        let pushed_at = now - Duration::seconds(3);
        assert!(!push_still_in_flight(now, Some(pushed_at), Some(now)));
    }

    #[test]
    fn push_still_in_flight_false_when_never_pushed() {
        let now = Utc::now();
        assert!(!push_still_in_flight(now, None, None));
    }
}
