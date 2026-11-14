use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::fleet_counts::SeatDecision;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub const POINTS: [&str; 3] = ["fleet_stop_check", "fleet_triage", "fleet_commit_message"];
pub const LIMIT: i64 = 500;

pub fn by_seat(rows: Vec<SeatDecision>) -> BTreeMap<String, BTreeMap<String, Vec<SeatDecision>>> {
    let mut seats: BTreeMap<String, BTreeMap<String, Vec<SeatDecision>>> = BTreeMap::new();
    for row in rows {
        seats
            .entry(row.seat.clone())
            .or_default()
            .entry(row.point.clone())
            .or_default()
            .push(row);
    }
    seats
}

pub async fn document(persist: &PersistHandle, fleet_id: i64, run_id: &str) -> Value {
    let points: Vec<String> = POINTS.iter().map(|p| (*p).to_owned()).collect();
    let rows = crate::llm::decisions::store(persist)
        .recent_fleet_decisions(fleet_id, &points, LIMIT)
        .await
        .unwrap_or_default();
    let triage = persist
        .fleet()
        .run_check_triage(run_id, LIMIT)
        .await
        .unwrap_or_default();
    json!({
        "basis": "the newest stop-check, check-triage and commit-message decisions Jev answered for this fleet, per seat and point, newest first; `verdict` is what the point composed and `applied` what it did. `check_triage` lists every triage line printed in this run's receipts, whether rules or Jev wrote it, with the seats that asked for the check",
        "limit": LIMIT,
        "by_seat": by_seat(rows),
        "check_triage": triage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, seat: &str, point: &str) -> SeatDecision {
        SeatDecision {
            id,
            created_at: chrono::DateTime::UNIX_EPOCH,
            seat: seat.to_owned(),
            point: point.to_owned(),
            applied: Some("chosen".to_owned()),
            fail_open: Some(false),
            verdict: None,
        }
    }

    #[test]
    fn by_seat_groups_rows_by_seat_then_point_keeping_their_order() {
        let grouped = by_seat(vec![
            row(3, "w1", "fleet_triage"),
            row(2, "w1", "fleet_stop_check"),
            row(1, "w1", "fleet_triage"),
            row(4, "w2", "fleet_stop_check"),
        ]);
        let w1 = &grouped["w1"];
        let ids: Vec<i64> = w1["fleet_triage"].iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![3, 1]);
        assert_eq!(w1["fleet_stop_check"].len(), 1);
        assert_eq!(grouped["w2"]["fleet_stop_check"][0].id, 4);
    }
}
