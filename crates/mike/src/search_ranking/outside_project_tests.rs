use serde_json::{Value, json};

use super::*;

fn project_row(reference: &str) -> Value {
    json!({"reference": reference, "provenance": {"tier": "operator"}})
}

fn outside_row(reference: &str) -> Value {
    let mut row = project_row(reference);
    mark_outside_project(&mut row);
    row
}

fn order(rows: Vec<(Value, Option<f32>)>) -> Vec<String> {
    weigh_outside_project(rows)
        .iter()
        .filter_map(|(row, _)| row["reference"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn weigh_outside_project_row_scored_2_above_a_project_row_stays_below_it() {
    let rows = vec![
        (outside_row("outside"), Some(2.0)),
        (project_row("inside"), Some(0.0)),
    ];

    assert_eq!(order(rows), vec!["inside", "outside"]);
}

#[test]
fn weigh_outside_project_row_scored_4_above_a_project_row_is_first() {
    let rows = vec![
        (outside_row("outside"), Some(4.0)),
        (project_row("inside"), Some(0.0)),
    ];

    assert_eq!(order(rows), vec!["outside", "inside"]);
}

#[test]
fn weigh_outside_project_reports_the_real_score_not_the_offset_one() {
    let rows = vec![
        (outside_row("outside"), Some(2.0)),
        (project_row("inside"), Some(0.5)),
    ];

    let weighed = weigh_outside_project(rows);

    assert_eq!(weighed[1].1, Some(2.0));
}

#[test]
fn weigh_outside_project_without_outside_rows_keeps_the_order_untouched() {
    let rows = vec![
        (project_row("a"), Some(0.1)),
        (project_row("b"), Some(0.9)),
        (project_row("c"), None),
    ];

    assert_eq!(order(rows), vec!["a", "b", "c"]);
}

#[test]
fn weigh_outside_project_keeps_unscored_rows_after_the_scored_ones() {
    let rows = vec![
        (project_row("inside"), Some(0.0)),
        (outside_row("outside"), Some(-5.0)),
        (project_row("unscored"), None),
    ];

    assert_eq!(order(rows), vec!["inside", "outside", "unscored"]);
}

#[test]
fn mark_outside_project_keeps_existing_provenance_and_creates_missing_provenance() {
    let mut with = project_row("a");
    let mut without = json!({"reference": "b"});

    mark_outside_project(&mut with);
    mark_outside_project(&mut without);

    assert_eq!(with["provenance"]["tier"], "operator");
    assert!(is_outside_project(&with));
    assert!(is_outside_project(&without));
    assert!(!is_outside_project(&project_row("c")));
}
