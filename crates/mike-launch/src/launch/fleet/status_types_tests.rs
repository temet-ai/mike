use super::*;

#[test]
fn a_fork_whose_first_request_wrote_more_than_it_read_is_cold() {
    let cache = first_request_cache(true, Some(33_000), Some(123_000));
    assert_eq!(cache.map(|c| c.cold), Some(true));
}

#[test]
fn a_fork_that_read_more_than_it_wrote_is_warm() {
    let cache = first_request_cache(true, Some(150_000), Some(1_200));
    assert_eq!(cache.map(|c| c.cold), Some(false));
}

#[test]
fn a_seat_that_is_not_a_fork_is_never_marked_cold() {
    let cache = first_request_cache(false, Some(0), Some(60_000));
    assert_eq!(cache.map(|c| c.cold), Some(false));
}

#[test]
fn a_seat_with_no_first_request_has_no_cache_reading() {
    assert_eq!(first_request_cache(true, None, None), None);
    assert_eq!(label(None), "");
}

#[test]
fn the_label_names_a_cold_start() {
    let cache = first_request_cache(true, Some(33_000), Some(123_000));
    assert_eq!(
        label(cache.as_ref()),
        "first request read 33k wrote 123k COLD START"
    );
}

fn facts(forked: bool, read: i64, written: i64) -> SeatFacts {
    SeatFacts {
        seat: Some("rust-developer-T3".into()),
        session: None,
        role: Some("rust-developer".into()),
        agent: None,
        model: None,
        opened_at: None,
        closed_at: None,
        requests: 1,
        last_request_at: None,
        context_tokens: None,
        live_state: None,
        last_status: None,
        first_request_cache: first_request_cache(forked, Some(read), Some(written)),
    }
}

#[test]
fn the_status_row_of_a_cold_fork_carries_the_cold_marker() {
    let header = FleetHeader {
        fleet: String::new(),
        slug: String::new(),
        state: "running".into(),
        run_id: None,
        started_at: None,
        finished_at: None,
        now: chrono::Utc::now(),
        missing: Vec::new(),
    };
    let seats = [facts(true, 33_000, 123_000), facts(true, 150_000, 500)];
    let def = crate::launch::fleet::definition::FleetDefinition::default();
    let status = crate::launch::fleet::status::build(header, &def, &seats, &[]);
    let cold: Vec<bool> = status
        .seats
        .iter()
        .map(|s| s.first_request_cache.is_some_and(|c| c.cold))
        .collect();
    assert_eq!(cold, vec![true, false]);
}
