use super::score::{
    EndItem, Variant, effective_label, frustration_rates, pairs, rule_precision,
    score_session_end_run, wilson_rate,
};

fn item(gold: &str, llm: Option<&str>, rule: Option<&str>) -> EndItem {
    EndItem {
        gold: gold.to_owned(),
        llm: llm.map(str::to_owned),
        rule_fired: rule.map(str::to_owned),
    }
}

#[test]
fn effective_label_rule_wins_only_in_variants_that_apply_it() {
    let i = item("fleet_seat", Some("unclear"), Some("fleet-seat-v1"));
    assert_eq!(effective_label(&i, Variant::ModelOnly), Some("unclear"));
    assert_eq!(effective_label(&i, Variant::FleetSeat), Some("fleet_seat"));
    assert_eq!(effective_label(&i, Variant::Done), Some("unclear"));
    assert_eq!(effective_label(&i, Variant::Both), Some("fleet_seat"));
}

#[test]
fn frustration_rates_count_true_and_false_positives() {
    let items = vec![
        item("closed_in_frustration", Some("closed_in_frustration"), None),
        item("done", Some("closed_in_frustration"), None),
        item("closed_in_frustration", Some("done"), None),
        item("done", Some("done"), None),
    ];
    let p = pairs(&items, Variant::ModelOnly);
    let (precision, recall) = frustration_rates(&p);
    assert_eq!((precision.k, precision.n), (1, 2));
    assert_eq!((recall.k, recall.n), (1, 2));
}

#[test]
fn rule_precision_counts_frustration_among_fires() {
    let items = vec![
        item("done", None, Some("done-v1")),
        item("done", None, Some("done-v1")),
        item("closed_in_frustration", None, Some("done-v1")),
        item("done", Some("done"), None),
    ];
    let (precision, frustrated) = rule_precision(&items, "done-v1");
    assert_eq!((precision.k, precision.n), (2, 3));
    assert_eq!((frustrated.k, frustrated.n), (1, 3));
}

#[test]
fn wilson_rate_brackets_the_point_estimate() {
    let r = wilson_rate(13, 14);
    let (Some(v), Some(low), Some(high)) = (r.value, r.low, r.high) else {
        panic!("rate {r:?} has no interval");
    };
    assert!(low < v && v < high);
    assert!(wilson_rate(0, 0).value.is_none());
}

#[test]
fn score_session_end_run_reports_four_variants() {
    let items = vec![
        item("done", Some("done"), Some("done-v1")),
        item("fleet_seat", Some("unclear"), Some("fleet-seat-v1")),
        item("closed_in_frustration", Some("closed_in_frustration"), None),
    ];
    let report = score_session_end_run(&items, 7);
    for key in ["model_only", "fleet_seat_v1", "done_v1", "both"] {
        assert!(
            report["variants"][key]["n"].as_u64().is_some(),
            "{key} missing"
        );
    }
    assert_eq!(report["items"], 3);
}
