use serde_json::json;

use super::gate::{NoiseBand, decide};
use super::known_answer::{KnownAnswer, judge_accuracy};
use super::numbers::FleetNumbers;
use crate::eval::gate::{EXIT_FAIL, EXIT_INCOMPLETE, EXIT_PASS};

fn record(time: f64, faults: f64, tokens: f64, manual: f64) -> FleetNumbers {
    FleetNumbers::from_record(&json!({
        "time_to_done_minutes": time,
        "fault_minutes": { "total": faults },
        "tokens_per_accepted_task": tokens,
        "manual_interventions": { "total": manual },
    }))
}

#[test]
fn after_inside_the_band_passes() {
    let before = record(100.0, 20.0, 1000.0, 5.0);
    let after = record(108.0, 21.0, 1050.0, 5.0);
    let decision = decide(&before, &after, &NoiseBand::Fraction(0.10));
    assert_eq!((decision.verdict, decision.exit_code), ("pass", EXIT_PASS));
}

#[test]
fn after_outside_the_band_fails_and_names_the_metric() {
    let before = record(100.0, 20.0, 1000.0, 5.0);
    let after = record(100.0, 30.0, 1000.0, 5.0);
    let decision = decide(&before, &after, &NoiseBand::Fraction(0.10));
    assert_eq!((decision.verdict, decision.exit_code), ("fail", EXIT_FAIL));
    assert_eq!(decision.report["worsened"], json!(["fault_minutes"]));
}

#[test]
fn an_improvement_passes() {
    let before = record(100.0, 20.0, 1000.0, 5.0);
    let after = record(50.0, 0.0, 500.0, 0.0);
    let decision = decide(&before, &after, &NoiseBand::Fraction(0.10));
    assert_eq!(decision.exit_code, EXIT_PASS);
}

#[test]
fn a_record_missing_a_number_is_incomplete() {
    let before = record(100.0, 20.0, 1000.0, 5.0);
    let after = FleetNumbers::from_record(&json!({ "time_to_done_minutes": 90.0 }));
    let decision = decide(&before, &after, &NoiseBand::Fraction(0.10));
    assert_eq!(decision.exit_code, EXIT_INCOMPLETE);
}

#[test]
fn a_new_manual_intervention_against_zero_fails() {
    let before = record(100.0, 20.0, 1000.0, 0.0);
    let after = record(100.0, 20.0, 1000.0, 1.0);
    let decision = decide(&before, &after, &NoiseBand::Fraction(0.10));
    assert_eq!(decision.exit_code, EXIT_FAIL);
}

#[test]
fn fewer_than_ten_runs_use_the_ten_percent_band() {
    let history = vec![record(100.0, 1.0, 1.0, 1.0); 9];
    assert_eq!(NoiseBand::from_history(&history), NoiseBand::Fraction(0.10));
}

#[test]
fn ten_runs_set_the_band_to_two_standard_deviations() {
    let history: Vec<FleetNumbers> = (0..10)
        .map(|i| record(100.0 + f64::from(i), 10.0, 1000.0, 2.0))
        .collect();
    let band = NoiseBand::from_history(&history);
    let before = record(100.0, 10.0, 1000.0, 2.0);
    let inside = record(105.0, 10.0, 1000.0, 2.0);
    let outside = record(110.0, 10.0, 1000.0, 2.0);
    assert_eq!(decide(&before, &inside, &band).exit_code, EXIT_PASS);
    assert_eq!(decide(&before, &outside, &band).exit_code, EXIT_FAIL);
}

#[test]
fn judge_accuracy_scores_verdicts_against_the_known_answer() {
    let landed = KnownAnswer {
        relanded: true,
        test_passed: true,
    };
    let missed = KnownAnswer {
        relanded: true,
        test_passed: false,
    };
    let score = judge_accuracy(&[
        (landed, "accepted"),
        (missed, "accepted"),
        (missed, "corrected"),
    ]);
    assert_eq!((score.judged, score.agreed), (3, 2));
    assert_eq!(judge_accuracy(&[]).accuracy, None);
}
