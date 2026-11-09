use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{BREAKER_BASE_COOLDOWN, BREAKER_MAX_COOLDOWN, BREAKER_THRESHOLD, COLD_AFTER_SECS};
use crate::circuit_breaker::CircuitBreaker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Ok,
    Loading,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RerankPhase {
    Off,
    Idle,
    Ready,
    Loading,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RerankStatus {
    pub(crate) phase: RerankPhase,
    pub(crate) url: String,
    pub(crate) model: String,
    pub(crate) breaker_open: bool,
    pub(crate) last_ok_at: Option<DateTime<Utc>>,
    pub(crate) last_error: Option<String>,
    pub(crate) last_error_at: Option<DateTime<Utc>>,
}

pub(super) struct Runtime {
    breaker: CircuitBreaker,
    last: Option<Outcome>,
    last_ok_at: Option<DateTime<Utc>>,
    last_error: Option<(DateTime<Utc>, String)>,
}

impl Runtime {
    pub(super) fn new() -> Self {
        Self {
            breaker: CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_BASE_COOLDOWN,
                BREAKER_MAX_COOLDOWN,
            ),
            last: None,
            last_ok_at: None,
            last_error: None,
        }
    }

    pub(super) fn breaker_open(&self, now: Instant) -> bool {
        self.breaker.should_skip(now)
    }

    pub(super) fn expects_cold_start(&self) -> bool {
        self.expects_cold_start_at(Utc::now())
    }

    pub(super) fn expects_cold_start_at(&self, now: DateTime<Utc>) -> bool {
        match self.last {
            None | Some(Outcome::Loading) => true,
            Some(Outcome::Failed) => false,
            Some(Outcome::Ok) => self
                .last_ok_at
                .is_none_or(|at| now - at > chrono::Duration::seconds(COLD_AFTER_SECS)),
        }
    }

    pub(super) fn record_ok(&mut self) {
        self.breaker.record(true, Instant::now());
        self.last = Some(Outcome::Ok);
        self.last_ok_at = Some(Utc::now());
        self.last_error = None;
    }

    pub(super) fn record_loading(&mut self) {
        self.last = Some(Outcome::Loading);
    }

    pub(super) fn record_failure(&mut self, message: String) {
        self.breaker.record(false, Instant::now());
        self.last = Some(Outcome::Failed);
        self.last_error = Some((Utc::now(), message));
    }

    pub(super) fn status(&self, enabled: bool, url: String, model: String) -> RerankStatus {
        let breaker_open = self.breaker_open(Instant::now());
        let phase = if !enabled {
            RerankPhase::Off
        } else if breaker_open {
            RerankPhase::Unavailable
        } else {
            match self.last {
                None => RerankPhase::Idle,
                Some(Outcome::Ok) => RerankPhase::Ready,
                Some(Outcome::Loading) => RerankPhase::Loading,
                Some(Outcome::Failed) => RerankPhase::Unavailable,
            }
        };
        RerankStatus {
            phase,
            url,
            model,
            breaker_open,
            last_ok_at: self.last_ok_at,
            last_error: self.last_error.as_ref().map(|(_, m)| m.clone()),
            last_error_at: self.last_error.as_ref().map(|(at, _)| *at),
        }
    }
}
