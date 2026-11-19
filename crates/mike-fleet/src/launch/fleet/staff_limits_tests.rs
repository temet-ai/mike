use super::{hold_text, memory_floor_gb, seat_room};
use crate::launch::fleet::test_fixtures::{changed, complete, parsed};

#[test]
fn with_no_staffing_limit_the_definition_adds_no_seat_cap_and_no_memory_floor() {
    let def = parsed(&complete());
    assert_eq!(seat_room(&def, 40, 5), usize::MAX);
    assert_eq!(memory_floor_gb(&def), None);
}

#[test]
fn max_seats_counts_live_and_starting_seats_against_the_limit() {
    let def = changed(serde_json::json!({"staffing": {"max_seats": 6}}));
    assert_eq!(seat_room(&def, 2, 1), 3);
    assert_eq!(seat_room(&def, 6, 0), 0);
    assert_eq!(seat_room(&def, 5, 4), 0);
}

#[test]
fn a_task_whose_seat_has_closed_does_not_count_against_max_seats() -> anyhow::Result<()> {
    let status = crate::launch::fleet::render::tests_fixture::fixture()?;
    assert_eq!(super::live_worker_seats(&status), 2);
    Ok(())
}

#[test]
fn reaching_max_seats_names_the_limit_not_the_terminal() {
    let def = changed(serde_json::json!({"staffing": {"max_seats": 2}}));
    let why = super::max_seats_reached(&def, 2, 0).unwrap_or_default();
    assert!(why.contains("staffing.max_seats is 2"), "{why}");
    assert_eq!(super::max_seats_reached(&def, 1, 0), None);
    assert_eq!(super::max_seats_reached(&parsed(&complete()), 9, 9), None);
    let (room, why) = super::binding_room(&def, (2, 0), (16, "surfaces".to_string()));
    assert_eq!(room, 0);
    assert!(why.contains("staffing.max_seats is 2"), "{why}");
    let (room, why) = super::binding_room(&def, (0, 0), (1, "surfaces".to_string()));
    assert_eq!((room, why.as_str()), (1, "surfaces"));
}

#[tokio::test]
async fn staffing_waits_when_the_host_has_less_free_memory_than_the_floor_and_is_unchanged_without_one()
 {
    let unset = parsed(&complete());
    assert_eq!(super::memory_hold(&unset).await, None);
    let impossible = changed(serde_json::json!({"staffing": {"min_free_memory_gb": 4_000_000}}));
    let held = super::memory_hold(&impossible).await;
    assert!(
        held.is_some_and(|text| text.contains("min_free_memory_gb is 4000000")),
        "a floor above any host's memory must hold staffing"
    );
    let trivial = changed(serde_json::json!({"staffing": {"min_free_memory_gb": 0}}));
    assert_eq!(super::memory_hold(&trivial).await, None);
}

#[test]
fn the_memory_floor_comes_from_the_definition_and_holds_staffing_at_or_below_it() {
    let def = changed(serde_json::json!({"staffing": {"min_free_memory_gb": 8}}));
    assert_eq!(memory_floor_gb(&def), Some(8.0));
    assert!(hold_text(8.0, Some(9.5)).is_none());
    let held = hold_text(8.0, Some(3.2)).unwrap_or_default();
    assert!(held.contains("3.2 GB"), "{held}");
    assert!(held.contains("min_free_memory_gb is 8"), "{held}");
    assert!(hold_text(8.0, None).is_none());
}

mod kit_host {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::launch::fleet::staff::{QUEUED, Waiting, startable, unstarted_tasks};
    use crate::launch::fleet::staff_claims::{annotate, set_waiting};
    use crate::launch::fleet::staff_limits::{
        accepted_open_seats, binding_room, live_worker_seats,
    };
    use crate::launch::fleet::status::{FleetStatus, SeatStatus, TaskStatus};
    use crate::launch::fleet::test_fixtures::changed;

    fn ids(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|id| (*id).to_string()).collect()
    }

    fn definition() -> crate::launch::fleet::definition::FleetDefinition {
        let task = |after: &[&str]| {
            serde_json::json!({"name": "kit", "discipline": "db-dev", "criteria": ["C1"],
                               "files": ["src/"], "brief": "Do it.", "after": after})
        };
        let producers = ["T1", "T2", "T3"];
        changed(serde_json::json!({
            "staffing": {"max_seats": 4},
            "tasks": {"T1": task(&[]), "T2": task(&[]), "T3": task(&[]),
                      "T4": task(&producers), "T5": task(&producers)}
        }))
    }

    fn status(run: &str) -> anyhow::Result<FleetStatus> {
        let mut status = crate::launch::fleet::render::tests_fixture::fixture()?;
        status.run_id = Some(run.to_string());
        let template_task = status.tasks[0].clone();
        let template_seat = status.seats[0].clone();
        let task = |id: &str, state: &str, ready: bool, seat: Option<String>| TaskStatus {
            id: id.to_string(),
            state: state.to_string(),
            ready,
            seat,
            ..template_task.clone()
        };
        status.tasks = ["T1", "T2", "T3"]
            .iter()
            .map(|id| task(id, "passed", true, Some(format!("db-dev-{id}"))))
            .chain(["T4", "T5"].iter().map(|id| task(id, QUEUED, false, None)))
            .collect();
        status.seats = (1..=3u128)
            .map(|n| SeatStatus {
                seat: format!("db-dev-T{n}"),
                session: Some(uuid::Uuid::from_u128(n)),
                closed: false,
                ..template_seat.clone()
            })
            .collect();
        Ok(status)
    }

    fn room(
        def: &crate::launch::fleet::definition::FleetDefinition,
        status: &FleetStatus,
    ) -> usize {
        binding_room(
            def,
            (live_worker_seats(status), 0),
            (usize::MAX, String::new()),
        )
        .0
    }

    #[test]
    fn dependents_of_three_accepted_producers_both_start_once_the_producers_seats_close()
    -> anyhow::Result<()> {
        let def = definition();
        let mut status = status("kit-host")?;
        let unstarted = unstarted_tasks(&status);
        assert_eq!(unstarted, ids(&["T4", "T5"]));

        let mut waiting = Waiting::default();
        let gated = startable(
            &def,
            &ids(&["T1", "T3"]),
            (&unstarted, &BTreeSet::new()),
            &mut waiting,
        );
        assert!(gated.is_empty());
        assert!(
            waiting
                .0
                .get("T4")
                .is_some_and(|why| why.contains("not released") && why.contains("T2"))
        );

        let released = ids(&["T1", "T2", "T3"]);
        let mut waiting = Waiting::default();
        let ready = startable(
            &def,
            &released,
            (&unstarted, &BTreeSet::new()),
            &mut waiting,
        );
        assert_eq!(ready, vec!["T4".to_string(), "T5".to_string()]);
        assert_eq!(
            room(&def, &status),
            1,
            "three idle producer seats hold three of four slots"
        );

        for (task, session) in accepted_open_seats(&status) {
            if let Some(seat) = status.seats.iter_mut().find(|s| s.session == Some(session)) {
                seat.closed = true;
            }
            if let Some(row) = status.tasks.iter_mut().find(|t| t.id == task) {
                row.seat = None;
            }
        }
        assert_eq!(live_worker_seats(&status), 0);
        assert!(
            room(&def, &status) >= ready.len(),
            "both dependents fit once the seats close"
        );
        Ok(())
    }

    #[test]
    fn a_queued_task_that_cannot_start_says_why_and_stays_staffable() -> anyhow::Result<()> {
        let run = "kit-host-reasons";
        let mut status = status(run)?;
        let mut reasons = BTreeMap::new();
        reasons.insert(
            "T4".to_string(),
            "held by a claim: a seat is being started for it".to_string(),
        );
        reasons.insert(
            "T5".to_string(),
            "no room: waiting: staffing.max_seats is 4".to_string(),
        );
        set_waiting(run, reasons);
        annotate(&mut status);
        let state = |id: &str| {
            status
                .tasks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.state.clone())
        };
        assert_eq!(
            state("T4").as_deref(),
            Some("queued: held by a claim: a seat is being started for it")
        );
        assert_eq!(
            state("T5").as_deref(),
            Some("queued: no room: waiting: staffing.max_seats is 4")
        );
        assert_eq!(state("T1").as_deref(), Some("passed"));
        assert_eq!(unstarted_tasks(&status), ids(&["T4", "T5"]));
        Ok(())
    }
}
