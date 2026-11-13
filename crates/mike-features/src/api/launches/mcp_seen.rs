use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::Path;
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use dashmap::DashMap;
use uuid::Uuid;

const KEEP: TimeDelta = TimeDelta::hours(24);

fn listed() -> &'static DashMap<Uuid, DateTime<Utc>> {
    static LISTED: OnceLock<DashMap<Uuid, DateTime<Utc>>> = OnceLock::new();
    LISTED.get_or_init(DashMap::new)
}

static REPORTING: AtomicBool = AtomicBool::new(false);

pub fn record(session: Uuid, at: DateTime<Utc>) {
    let map = listed();
    map.retain(|_, seen| at.signed_duration_since(*seen) < KEEP);
    map.insert(session, at);
    REPORTING.store(true, Ordering::Relaxed);
}

pub fn tools_listed(session: Uuid) -> bool {
    listed().contains_key(&session)
}

pub fn reporting() -> bool {
    REPORTING.load(Ordering::Relaxed)
}

pub async fn tools_listed_by(Path(session): Path<Uuid>) -> StatusCode {
    record(session, Utc::now());
    StatusCode::NO_CONTENT
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn a_report_is_seen_and_old_reports_are_pruned() {
        let (old, fresh) = (Uuid::new_v4(), Uuid::new_v4());
        let now = Utc::now();
        record(old, now - TimeDelta::hours(30));
        assert!(tools_listed(old));
        record(fresh, now);
        assert!(tools_listed(fresh));
        assert!(!tools_listed(old));
        assert!(reporting());
    }
}
