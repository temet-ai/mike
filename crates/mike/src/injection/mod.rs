mod admission;
mod policy;

#[cfg(test)]
mod gate_tests;

use std::time::{Duration, Instant};

use dashmap::DashMap;

use crate::store::RequestStore;
use crate::store::models::{ReminderSource, SELF_IDENTITY_ID_LINE, SystemReminder};
use crate::wire::manifest::sha256_hex;

pub(crate) use admission::{ALREADY_IN_REQUEST, Admission, DUPLICATE_CONTENT};
pub(crate) use policy::{InjectionPolicy, RepeatRule, policy_for};

const SWEEP_THRESHOLD: usize = 1000;

const STALE_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
pub(crate) struct InjectionGate {
    delivered: DashMap<String, Instant>,
    woke: DashMap<String, Instant>,
}

impl InjectionGate {
    pub(crate) fn admit(&self, session_id: &str, reminder: &SystemReminder) -> Admission {
        self.admit_for_request(session_id, reminder, &[])
    }

    pub(crate) fn admit_for_request(
        &self,
        session_id: &str,
        reminder: &SystemReminder,
        body_bytes: &[u8],
    ) -> Admission {
        let policy = policy_for(&reminder.source);
        match policy.repeat {
            RepeatRule::OncePerSessionPerContent => {
                if self
                    .delivered
                    .contains_key(&content_key(session_id, &policy, &reminder.payload))
                {
                    return Admission::Suppressed(DUPLICATE_CONTENT);
                }
            }
            RepeatRule::WhenAbsentFromRequest => {
                if marker_present_in_request(reminder, body_bytes) {
                    return Admission::Suppressed(ALREADY_IN_REQUEST);
                }
            }
            RepeatRule::Always => {}
        }
        if !policy.may_wake {
            return Admission::DeliverAppendOnly;
        }
        match policy.min_wake_interval {
            Some(window) if self.woke_within(session_id, &policy, window) => {
                Admission::DeliverWakeThrottled
            }
            _ => Admission::DeliverAndWake,
        }
    }

    pub(crate) fn record_delivery(&self, session_id: &str, reminder: &SystemReminder) -> bool {
        let policy = policy_for(&reminder.source);
        if policy.repeat != RepeatRule::OncePerSessionPerContent {
            return true;
        }
        sweep_stale(&self.delivered);
        self.delivered
            .insert(
                content_key(session_id, &policy, &reminder.payload),
                Instant::now(),
            )
            .is_none()
    }

    pub(crate) fn record_wake(&self, session_id: &str, reminder: &SystemReminder) {
        let policy = policy_for(&reminder.source);
        if policy.min_wake_interval.is_none() {
            return;
        }
        sweep_stale(&self.woke);
        self.woke
            .insert(wake_key(session_id, &policy), Instant::now());
    }

    fn woke_within(&self, session_id: &str, policy: &InjectionPolicy, window: Duration) -> bool {
        self.woke
            .get(&wake_key(session_id, policy))
            .is_some_and(|last| last.elapsed() < window)
    }

    #[cfg(test)]
    fn backdate_wake(&self, session_id: &str, reminder: &SystemReminder, age: Duration) {
        let policy = policy_for(&reminder.source);
        self.woke.insert(
            wake_key(session_id, &policy),
            Instant::now() - age - Duration::from_secs(1),
        );
    }
}

pub(crate) fn enqueue_if_admitted(
    gate: &InjectionGate,
    store: &mut RequestStore,
    session_id: &str,
    reminder: SystemReminder,
) -> bool {
    if let Some(reason) = gate.admit(session_id, &reminder).suppression_reason() {
        tracing::debug!(
            session_id,
            reason,
            source = ?reminder.source,
            "injection gate refused a reminder before it reached the queue"
        );
        return false;
    }
    store.enqueue_reminder(session_id, reminder);
    true
}

fn marker_present_in_request(reminder: &SystemReminder, body_bytes: &[u8]) -> bool {
    if !matches!(reminder.source, ReminderSource::SelfIdentity) {
        return false;
    }
    let Some(id_line) = reminder
        .payload
        .lines()
        .find(|line| line.starts_with(SELF_IDENTITY_ID_LINE))
    else {
        return false;
    };
    std::str::from_utf8(body_bytes).is_ok_and(|body| body.contains(id_line))
}

fn content_key(session_id: &str, policy: &InjectionPolicy, payload: &str) -> String {
    format!(
        "{session_id}\u{1f}{}\u{1f}{}",
        policy.source_kind,
        sha256_hex(payload.as_bytes())
    )
}

fn wake_key(session_id: &str, policy: &InjectionPolicy) -> String {
    format!("{session_id}\u{1f}{}", policy.source_kind)
}

fn sweep_stale(map: &DashMap<String, Instant>) {
    if map.len() <= SWEEP_THRESHOLD {
        return;
    }
    map.retain(|_, last| last.elapsed() < STALE_AFTER);
}
