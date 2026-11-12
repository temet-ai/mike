use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::models::{InterceptedRequest, RequestId};

#[derive(Debug, Clone)]
struct Deferred {
    session_id: String,
    request_at: DateTime<Utc>,
    reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct UnrecordedCapture {
    pub(crate) session_id: String,
    pub(crate) request_at: DateTime<Utc>,
    pub(crate) given_up_at: DateTime<Utc>,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WaitingCapture {
    pub(crate) session_id: String,
    pub(crate) request_at: DateTime<Utc>,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CaptureRetryHealth {
    pub(crate) waiting_count: usize,
    pub(crate) recorded_late_count: u64,
    pub(crate) unrecorded_count: usize,
    pub(crate) waiting: Vec<WaitingCapture>,
    pub(crate) unrecorded: Vec<UnrecordedCapture>,
}

#[derive(Default)]
struct Ledger {
    waiting: HashMap<u64, Deferred>,
    recorded_late: u64,
    unrecorded: Vec<UnrecordedCapture>,
}

static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();

fn with_ledger<T>(f: impl FnOnce(&mut Ledger) -> T) -> T {
    let ledger = LEDGER.get_or_init(Mutex::default);
    let mut guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub(crate) fn defer(id: RequestId, req: &InterceptedRequest, reason: &str) {
    let entry = Deferred {
        session_id: req.session_id.clone(),
        request_at: req.created_at,
        reason: reason.to_string(),
    };
    with_ledger(|l| l.waiting.insert(id.as_u64(), entry));
}

pub(crate) fn is_deferred(id: RequestId) -> bool {
    with_ledger(|l| l.waiting.contains_key(&id.as_u64()))
}

pub(super) fn note_recorded_late(id: RequestId) {
    with_ledger(|l| {
        if l.waiting.remove(&id.as_u64()).is_some() {
            l.recorded_late = l.recorded_late.saturating_add(1);
        }
    });
}

pub(super) fn note_gave_up(id: RequestId, reason: &str) {
    let removed = with_ledger(|l| l.waiting.remove(&id.as_u64()));
    if let Some(deferred) = removed {
        note_unrecorded(&deferred.session_id, deferred.request_at, reason);
    }
}

pub(crate) fn note_unrecorded(session_id: &str, request_at: DateTime<Utc>, reason: &str) {
    let entry = UnrecordedCapture {
        session_id: session_id.to_string(),
        request_at,
        given_up_at: Utc::now(),
        reason: reason.to_string(),
    };
    tracing::error!(
        session_id = %session_id,
        request_at = %request_at,
        reason,
        "request could not be recorded; counted in /api/health unrecorded_captures"
    );
    with_ledger(|l| l.unrecorded.push(entry));
}

pub(crate) fn health() -> CaptureRetryHealth {
    with_ledger(|l| {
        let mut waiting: Vec<WaitingCapture> = l
            .waiting
            .values()
            .map(|d| WaitingCapture {
                session_id: d.session_id.clone(),
                request_at: d.request_at,
                reason: d.reason.clone(),
            })
            .collect();
        waiting.sort_by(|a, b| (a.request_at, &a.session_id).cmp(&(b.request_at, &b.session_id)));
        CaptureRetryHealth {
            waiting_count: waiting.len(),
            recorded_late_count: l.recorded_late,
            unrecorded_count: l.unrecorded.len(),
            waiting,
            unrecorded: l.unrecorded.clone(),
        }
    })
}
