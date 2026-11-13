use serde::Deserialize;

use super::metrics::{
    WeightedItem, agreement, class_counts, confusion, gold_delivery, kappa, weighted_rates,
};
use super::report::{ScoreItem, score_run};

#[derive(Deserialize)]
struct FixtureItem {
    gold_reaction: String,
    haiku_reaction: String,
    sonnet_reaction: String,
}

#[derive(Deserialize)]
struct Fixture {
    items: Vec<FixtureItem>,
}

fn fixture() -> anyhow::Result<Fixture> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/eval/fleet_mirror_w4_d1.json");
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

#[test]
fn metrics_reproduce_fleet_mirror_d1_haiku_kappa() -> anyhow::Result<()> {
    let f = fixture()?;
    let pairs: Vec<(&str, &str)> = f
        .items
        .iter()
        .map(|i| (i.gold_reaction.as_str(), i.haiku_reaction.as_str()))
        .collect();
    assert_eq!(pairs.len(), 200);
    assert_eq!(agreement(&pairs), 164);
    let k = kappa(&pairs).unwrap_or_default();
    assert!((k - 0.7429).abs() < 5e-5, "{k}");
    Ok(())
}

#[test]
fn metrics_reproduce_fleet_mirror_d1_sonnet_kappa() -> anyhow::Result<()> {
    let f = fixture()?;
    let pairs: Vec<(&str, &str)> = f
        .items
        .iter()
        .map(|i| (i.gold_reaction.as_str(), i.sonnet_reaction.as_str()))
        .collect();
    assert_eq!(agreement(&pairs), 168);
    let k = kappa(&pairs).unwrap_or_default();
    assert!((k - 0.7707).abs() < 5e-5, "{k}");
    Ok(())
}

#[test]
fn metrics_per_class_counts_match_confusion_matrix() {
    let pairs = [
        ("corrected", "corrected"),
        ("corrected", "accepted"),
        ("accepted", "corrected"),
        ("accepted", "accepted"),
    ];
    let c = class_counts(&pairs, "corrected");
    let m = confusion(&pairs);
    assert_eq!(c.tp, m["corrected"]["corrected"]);
    assert_eq!(c.false_negatives, m["corrected"]["accepted"]);
    assert_eq!(c.fp, m["accepted"]["corrected"]);
}

fn items(n: usize) -> Vec<ScoreItem> {
    (0..n)
        .map(|i| ScoreItem {
            turn_id: i as i64,
            gold_reaction: if i % 3 == 0 {
                "corrected".to_owned()
            } else {
                "accepted".to_owned()
            },
            gold_delivery: Some("delivered".to_owned()),
            pred_reaction: Some(if i % 4 == 0 {
                "corrected".to_owned()
            } else {
                "accepted".to_owned()
            }),
            pred_delivery: Some("delivered".to_owned()),
            stratum: if i % 2 == 0 {
                "a".to_owned()
            } else {
                "b".to_owned()
            },
            inclusion_prob: Some(0.5),
        })
        .collect()
}

#[test]
fn bootstrap_is_reproducible_for_a_seed() {
    let set = items(60);
    assert_eq!(score_run(&set, false, 41), score_run(&set, false, 41));
}

#[test]
fn bootstrap_ci_contains_point_estimate() {
    let set = items(90);
    let report = score_run(&set, false, 7);
    let f1 = report["corrected"]["f1"].as_f64().unwrap_or_default();
    let low = report["corrected"]["f1_ci"][0].as_f64().unwrap_or(1.0);
    let high = report["corrected"]["f1_ci"][1].as_f64().unwrap_or(0.0);
    assert!(low <= f1 && f1 <= high, "{low} {f1} {high}");
}

#[test]
fn weighted_recall_equals_unweighted_when_inclusion_is_uniform() {
    let set: Vec<WeightedItem> = [(true, true), (true, false), (false, true), (true, true)]
        .iter()
        .map(|(p, g)| WeightedItem {
            predicted: *p,
            positive: *g,
            inclusion_prob: Some(0.25),
        })
        .collect();
    let w = weighted_rates(&set);
    assert!((w.recall.unwrap_or_default() - 2.0 / 3.0).abs() < 1e-9);
    assert!((w.precision.unwrap_or_default() - 2.0 / 3.0).abs() < 1e-9);
}

#[test]
fn weighted_scores_exclude_items_with_null_inclusion_prob_and_count_them() {
    let set = [
        WeightedItem {
            predicted: true,
            positive: true,
            inclusion_prob: Some(0.5),
        },
        WeightedItem {
            predicted: true,
            positive: false,
            inclusion_prob: None,
        },
    ];
    let w = weighted_rates(&set);
    assert_eq!(w.excluded_null_weight, 1);
    assert_eq!(w.precision, Some(1.0));
}

#[test]
fn r3_baseline_is_scored_on_binary_corrected_only() {
    let mut set = items(12);
    set[1].pred_reaction = Some("abandoned".to_owned());
    let report = score_run(&set, true, 3);
    assert!(report["delivery"].is_null());
    assert!(report["reaction"]["per_class"].get("other").is_some());
    assert!(report["reaction"]["per_class"].get("redirected").is_none());
}

#[test]
fn gold_delivered_with_irreversible_action_scores_as_shipped() {
    assert_eq!(gold_delivery("delivered", true), "shipped");
    assert_eq!(gold_delivery("delivered", false), "delivered");
    assert_eq!(gold_delivery("partial", true), "partial");
}
