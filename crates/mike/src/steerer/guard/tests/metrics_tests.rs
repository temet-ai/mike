use super::metrics::{audit, miss_weight, question_metrics, recall_of, wilson};
use crate::store::persistence::steering::guard::GuardMetricRow;

fn row(question: &str, trigger: &str, fired: bool, label: Option<&str>) -> GuardMetricRow {
    GuardMetricRow {
        uuid: uuid::Uuid::new_v4(),
        session_id: 1,
        operator_turn_id: 1,
        question: Some(question.to_owned()),
        trigger_point: Some(trigger.to_owned()),
        status: if fired {
            "resolved_correction"
        } else {
            "resolved_pass"
        }
        .to_owned(),
        outcome_label: label.map(str::to_owned),
        exemption: None,
        arm: None,
        gate_open: true,
        gate_reason: None,
        other_g2_open: false,
        other_g2b_open: false,
        abstention: None,
        recall_sample: false,
        first_vote_fires: Some(fired),
        stream_open: false,
        channel_live: false,
        next_genuine_after_secs: None,
        superseded: false,
    }
}

#[test]
fn wilson_interval_known_values() {
    let i = wilson(8.0, 10.0);
    assert_eq!(i.value, Some(0.8));
    let (Some(low), Some(high)) = (i.low, i.high) else {
        panic!("no interval for {i:?}");
    };
    assert!((low - 0.490).abs() < 0.01, "{low}");
    assert!((high - 0.943).abs() < 0.01, "{high}");
}

#[test]
fn recall_weights_sampled_misses_by_inverse_pct() {
    let right = row("relapse", "turn_end", true, Some("right"));
    let mut sampled_miss = row("relapse", "turn_end", false, Some("wrong"));
    sampled_miss.gate_open = false;
    sampled_miss.recall_sample = true;
    assert!((miss_weight(&sampled_miss, 10) - 10.0).abs() < f64::EPSILON);
    let recall = recall_of(&[&right, &sampled_miss], 10);
    assert_eq!(recall, Some(1.0 / 11.0));
}

#[test]
fn precision_null_on_zero_denominator() {
    let metrics = question_metrics(&[row("unobserved_done", "turn_end", false, None)], 10);
    assert_eq!(metrics[0].precision.value, None);
}

#[test]
fn daily_cap_rows_excluded_from_recall_denominator() {
    let right = row("relapse", "turn_end", true, Some("right"));
    let mut capped = row("relapse", "turn_end", false, Some("wrong"));
    capped.abstention = Some("daily_cap".to_owned());
    assert_eq!(recall_of(&[&right, &capped], 10), Some(1.0));
    let metrics = question_metrics(&[right, capped], 10);
    assert_eq!(metrics[0].daily_cap_excluded, 1);
}

#[test]
fn metrics_split_by_trigger_point_and_arm() {
    let mut g2 = row("unbacked_wait", "turn_end", true, Some("right"));
    g2.arm = Some("g2".to_owned());
    let mut g2b = row("unbacked_wait", "quiescence", true, Some("wrong"));
    g2b.arm = Some("g2b".to_owned());
    let metrics = question_metrics(&[g2, g2b], 10);
    assert_eq!(metrics.len(), 2);
    let quiet = metrics.iter().find(|m| m.trigger_point == "quiescence");
    assert_eq!(quiet.map(|m| m.funnel.gate_open_g2b), Some(1));
}

#[test]
fn audit_counts_fires_on_audit_set_sessions() {
    let mut fire = row("relapse", "turn_end", true, None);
    fire.session_id = 42;
    let report = audit(&[fire.clone()], &[42, 43]);
    assert_eq!(report.audit_set_fires, 1);
    assert_eq!(report.offending_evaluations, vec![fire.uuid]);
    assert_eq!(report.audit_set_sessions, 2);
}
