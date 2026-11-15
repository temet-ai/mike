use std::collections::BTreeMap;

use anyhow::Result;

use super::queries::{ArmRow, fold_arm_chunks};
use super::stats::{median, per_lane, per_turn, permutation_p, roll_up, weighted};
use super::test_support::{arms, usage, usage_row, who};
use super::types::Lane;

#[test]
fn per_turn_missing_usage_is_excluded_instead_of_free() {
    let assigned = arms(&[(1, "forward")]);
    let cost = BTreeMap::from([(1, usage(0, false))]);
    let turns = BTreeMap::from([(1, 3)]);
    assert!(per_turn(&assigned, &cost, &turns, "forward").is_empty());
}

#[test]
fn per_turn_divides_weighted_cost_by_operator_turns() {
    let assigned = arms(&[(1, "forward"), (2, "control")]);
    let cost = BTreeMap::from([(1, usage(600, true)), (2, usage(600, true))]);
    let turns = BTreeMap::from([(1, 3), (2, 0)]);
    assert_eq!(per_turn(&assigned, &cost, &turns, "forward"), vec![200.0]);
    assert!(per_turn(&assigned, &cost, &turns, "control").is_empty());
}

#[test]
fn roll_up_splits_usage_per_session_and_per_subagent_lane() -> Result<()> {
    let primary = who(1, None, None, "primary");
    let sub = |agent: &str| who(1, None, Some(agent), "subagent");
    let rows = vec![
        usage_row(primary, 10, true),
        usage_row(sub("a"), 5, true),
        usage_row(sub("b"), 7, false),
    ];
    let (cost, lanes) = roll_up(&rows);
    let total = cost
        .get(&1)
        .ok_or_else(|| anyhow::anyhow!("no session 1"))?;
    assert_eq!(total.fresh, 22);
    assert!(!total.complete_usage);
    let keys: Vec<Lane> = lanes.keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            Lane {
                session_id: 1,
                agent_instance_id: None,
                claude_code_agent_id: Some("a".to_string())
            },
            Lane {
                session_id: 1,
                agent_instance_id: None,
                claude_code_agent_id: Some("b".to_string())
            },
        ]
    );
    assert_eq!(
        per_lane(&arms(&[(1, "forward")]), &lanes, "forward"),
        vec![5.0]
    );
    Ok(())
}

#[test]
fn weighted_applies_the_billing_weights() {
    let mut row = usage(100, true);
    row.cache_write = 100;
    row.cache_read = 100;
    row.output = 100;
    assert!((weighted(&row) - 100.0 * (1.0 + 1.25 + 0.1 + 5.0)).abs() < 1e-9);
}

#[test]
fn permutation_p_separates_clear_from_noise() {
    let high: Vec<f64> = (0..40).map(|i| 200.0 + f64::from(i)).collect();
    let low: Vec<f64> = (0..40).map(|i| 100.0 + f64::from(i)).collect();
    assert!(permutation_p(&high, &low) < 0.01);
    assert!(permutation_p(&low, &low.clone()) > 0.5);
}

#[test]
fn permutation_p_is_reproducible_and_one_without_data() {
    let a: Vec<f64> = (0..30).map(|i| 100.0 + f64::from(i % 7)).collect();
    let b: Vec<f64> = (0..30).map(|i| 101.0 + f64::from(i % 5)).collect();
    assert_eq!(permutation_p(&a, &b), permutation_p(&a, &b));
    assert_eq!(permutation_p(&[], &b), 1.0);
    assert_eq!(permutation_p(&a, &[]), 1.0);
}

#[test]
fn median_averages_the_middle_pair_of_an_even_count() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
}

fn arm_row(session: i64, arm: &str) -> ArmRow {
    ArmRow {
        session_id: session,
        arm: Some(arm.to_string()),
        observed_arms: 1,
    }
}

#[test]
fn fold_arm_chunks_excludes_sessions_whose_arm_changes_across_days() {
    let chunks = vec![
        vec![arm_row(1, "control")],
        vec![arm_row(1, "forward"), arm_row(2, "control")],
    ];
    let (assigned, mixed) = fold_arm_chunks(chunks);
    assert_eq!(assigned, arms(&[(2, "control")]));
    assert_eq!(mixed, 1);
}

#[test]
fn fold_arm_chunks_excludes_sessions_that_mix_arms_within_a_day() {
    let mut row = arm_row(1, "control");
    row.observed_arms = 2;
    let (assigned, mixed) = fold_arm_chunks(vec![vec![row, arm_row(3, "forward")]]);
    assert_eq!(assigned, arms(&[(3, "forward")]));
    assert_eq!(mixed, 1);
}
