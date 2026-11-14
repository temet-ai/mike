use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use super::{Inputs, LaneInput, planned_operation, report};
use crate::launch::fleet::build_waits::BuildWaits;
use crate::launch::fleet::managed_queue::{Enlisted, Lane, Waiting, enlist, leave, waiting_in};
use crate::launch::fleet::owned_execution::registry;
use crate::launch::fleet::test_fixtures::{changed, complete, parsed};

fn waiting(receipts: &[i64]) -> Vec<Waiting> {
    receipts
        .iter()
        .map(|receipt| Waiting {
            receipt: *receipt,
            seat: format!("seat-{receipt}"),
        })
        .collect()
}

#[test]
fn the_report_counts_seats_builds_and_memory_and_places_each_queued_check_in_line() {
    let def = changed(json!({"staffing": {"max_seats": 12, "min_free_memory_gb": 6}}));
    let slots = Lane::Check.slots();
    let capacity = report(Inputs {
        def: &def,
        seats_live: 9,
        seats_starting: 2,
        free_memory_gb: Some(7.46),
        lanes: vec![LaneInput {
            lane: Lane::Check,
            command: "scripts/cx check".to_string(),
            executing: 2,
            waiting: waiting(&[11, 12, 13]),
            measured: Some(Duration::from_secs(120)),
        }],
        waits: BuildWaits::default(),
    });

    assert_eq!(capacity.seats_live, 9);
    assert_eq!(capacity.seats_starting, 2);
    assert_eq!(capacity.seat_limit, Some(12));
    assert_eq!(capacity.min_free_memory_gb, Some(6));
    assert_eq!(capacity.free_memory_gb, Some(7.5));
    assert_eq!(capacity.builds_in_flight, 2);
    assert_eq!(capacity.builds_queued, 3);
    assert_eq!(capacity.lanes[0].lane, "check");
    assert_eq!(capacity.lanes[0].slots, slots);
    assert_eq!(capacity.lanes[0].measured_run_seconds, Some(120));
    let places: Vec<usize> = capacity.queue.iter().map(|queued| queued.place).collect();
    assert_eq!(places, vec![1, 2, 3]);
    let receipts: Vec<i64> = capacity.queue.iter().map(|queued| queued.receipt).collect();
    assert_eq!(receipts, vec![11, 12, 13]);
    for (ahead, queued) in capacity.queue.iter().enumerate() {
        let rounds = u64::try_from((2 + ahead) / slots).unwrap_or(u64::MAX);
        assert_eq!(queued.expected_wait_seconds, Some(120 * rounds));
    }
}

#[test]
fn a_check_with_no_measured_run_time_has_a_place_and_no_invented_wait() {
    let def = parsed(&complete());
    let capacity = report(Inputs {
        def: &def,
        seats_live: 0,
        seats_starting: 0,
        free_memory_gb: None,
        lanes: vec![LaneInput {
            lane: Lane::Gate,
            command: "make gate".to_string(),
            executing: 0,
            waiting: waiting(&[5]),
            measured: None,
        }],
        waits: BuildWaits::default(),
    });
    assert_eq!(capacity.seat_limit, None);
    assert_eq!(capacity.min_free_memory_gb, None);
    assert_eq!(capacity.free_memory_gb, None);
    assert_eq!(capacity.queue[0].place, 1);
    assert_eq!(capacity.queue[0].expected_wait_seconds, None);
    assert!(capacity.measured_builds.is_none());
}

#[test]
fn measured_build_waits_are_reported_with_the_wait_per_compile_minute() -> anyhow::Result<()> {
    let def = parsed(&complete());
    let capacity = report(Inputs {
        def: &def,
        seats_live: 3,
        seats_starting: 0,
        free_memory_gb: Some(20.0),
        lanes: Vec::new(),
        waits: BuildWaits {
            commands: 50,
            lock_wait_minutes: 800.0,
            unit_wait_minutes: 160.0,
            compile_minutes: 200.0,
        },
    });
    let Some(measured) = capacity.measured_builds else {
        anyhow::bail!("no measured builds were reported");
    };
    assert_eq!(measured.commands, 50);
    assert_eq!(measured.compile_minutes, 200.0);
    assert_eq!(measured.wait_minutes_per_compile_minute, Some(4.8));
    Ok(())
}

#[test]
fn the_planned_operation_states_the_queue_when_every_seat_runs_it_once() {
    let def = changed(json!({"execution": {"operations": {"build": {
        "command": "scripts/cx check", "purpose": "compile",
        "resources": [{"name": "rust-build", "capacity": 2}]
    }}}}));
    let planned = planned_operation(&def, "scripts/cx check", Some(Duration::from_secs(300)), 12);
    assert_eq!(planned.capacity, Some(2));
    assert_eq!(planned.measured_run_seconds, Some(300));
    assert_eq!(planned.queue_minutes_if_every_seat_runs_it_once, Some(30.0));

    let undeclared = planned_operation(&def, "make test", Some(Duration::from_secs(300)), 12);
    assert_eq!(undeclared.capacity, None);
    assert_eq!(undeclared.queue_minutes_if_every_seat_runs_it_once, None);
}

#[tokio::test]
async fn the_live_queue_lists_waiting_receipts_oldest_first() -> anyhow::Result<()> {
    let repo = format!("/nonexistent/capacity-{}", Uuid::new_v4());
    let fence = registry().fence(&repo).await;
    let first = fence.register("a").map_err(anyhow::Error::msg)?;
    let second = fence.register("b").map_err(anyhow::Error::msg)?;
    drop(fence);
    for (receipt, seat, owner) in [
        (9_100_002, "seat-b", second.owner()),
        (9_100_001, "seat-a", first.owner()),
    ] {
        enlist(
            receipt,
            &Enlisted {
                repo: &repo,
                lane: Lane::Check,
                ledger: &repo,
                seat,
                state: "c1",
            },
            owner,
        );
    }

    let (executing, queued) = waiting_in(&repo, Lane::Check);
    assert_eq!(executing, 0);
    assert_eq!(
        queued
            .iter()
            .map(|w| (w.receipt, w.seat.as_str()))
            .collect::<Vec<_>>(),
        vec![(9_100_001, "seat-a"), (9_100_002, "seat-b")]
    );
    assert!(waiting_in(&repo, Lane::Gate).1.is_empty());
    leave(Some(9_100_001));
    leave(Some(9_100_002));
    first.receipt_recorded();
    second.receipt_recorded();
    Ok(())
}
