use uuid::Uuid;

use super::holdback::{HOLDBACK_SHARE, draw, sent_anyway};

#[test]
fn holdback_draw_same_evaluation_same_draw_in_unit_range() {
    let evaluation = Uuid::from_u128(0x5eed_0305);
    let first = draw(evaluation);
    assert!((first - draw(evaluation)).abs() < f64::EPSILON);
    assert!((0.0..1.0).contains(&first));
    assert_eq!(sent_anyway(Some(evaluation)), first < HOLDBACK_SHARE);
}

#[test]
fn holdback_over_many_evaluations_sends_about_ten_percent() {
    let total = 20_000_u32;
    let sent = (0..total)
        .map(|i| Uuid::from_u128(u128::from(i) * 7_919 + 11))
        .filter(|uuid| sent_anyway(Some(*uuid)))
        .count();
    let share = sent as f64 / f64::from(total);
    assert!((0.09..0.11).contains(&share), "share {share}");
}

#[test]
fn holdback_without_an_evaluation_never_sends() {
    assert!(!sent_anyway(None));
}
