use serde_json::{Value, json};

use super::wide::merge;

fn row(reference: &str, rank: f64) -> Value {
    json!({"reference": reference, "rank": rank})
}

fn references(rows: &[Value]) -> Vec<&str> {
    rows.iter()
        .filter_map(|row| row["reference"].as_str())
        .collect()
}

#[test]
fn merge_orders_both_projects_by_fused_rank() {
    let own = vec![row("turn:1", 0.03), row("turn:2", 0.01)];
    let extra = vec![row("turn:9", 0.02)];

    let merged = merge(own, extra, 10);

    assert_eq!(references(&merged), vec!["turn:1", "turn:9", "turn:2"]);
}

#[test]
fn merge_keeps_the_first_copy_of_a_reference_and_cuts_to_wanted() {
    let own = vec![row("lesson:a", 0.03), row("turn:2", 0.02)];
    let extra = vec![row("lesson:a", 0.05), row("turn:9", 0.01)];

    let merged = merge(own, extra, 2);

    assert_eq!(references(&merged), vec!["lesson:a", "turn:2"]);
    assert_eq!(merged[0]["rank"], json!(0.03));
}

#[test]
fn merge_with_nothing_from_the_extra_project_keeps_the_own_order() {
    let own = vec![row("turn:1", 0.03), row("turn:2", 0.01)];

    let merged = merge(own, Vec::new(), 10);

    assert_eq!(references(&merged), vec!["turn:1", "turn:2"]);
}
