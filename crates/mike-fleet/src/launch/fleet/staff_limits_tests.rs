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
