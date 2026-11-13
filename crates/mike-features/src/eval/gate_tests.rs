use std::collections::HashMap;

use serde::Deserialize;

use super::gate::{
    EXIT_FAIL, EXIT_INCOMPLETE, EXIT_PASS, EXIT_REFUSED, Floors, GateItem, GateRequest, GateRule,
    GateRun, decide,
};

fn item(turn_id: i64, gold: &str, pred: &str) -> GateItem {
    GateItem {
        turn_id,
        outcome: "judged".to_owned(),
        gold_reaction: gold.to_owned(),
        gold_delivery: Some("delivered".to_owned()),
        pred_reaction: Some(pred.to_owned()),
        pred_delivery: Some("delivered".to_owned()),
        inclusion_prob: None,
        control: false,
        settled_without_reply: false,
    }
}

fn run(id: i64, split: &str, items: Vec<GateItem>) -> GateRun {
    GateRun {
        run_id: id,
        status: "complete".to_owned(),
        label_ids_hash: "h".to_owned(),
        split: split.to_owned(),
        holdout_batch: (split == "holdout").then(|| "jc11/batch-1".to_owned()),
        repeat_of_run_id: None,
        r3: false,
        slices: Vec::new(),
        items,
    }
}

fn perfect(n: i64) -> Vec<GateItem> {
    (0..n)
        .map(|i| {
            if i % 3 == 0 {
                item(i, "corrected", "corrected")
            } else {
                item(i, "accepted", "accepted")
            }
        })
        .collect()
}

fn request<'a>(c: &'a GateRun, b: &'a GateRun) -> GateRequest<'a> {
    GateRequest {
        rule: GateRule::Promotion,
        candidate: c,
        baseline: b,
        noise: None,
        prior_holdout_gates: 0,
        override_reason: None,
        floors: Floors::default(),
    }
}

#[test]
fn gate_with_unequal_label_hash_is_incomplete() {
    let c = run(1, "holdout", perfect(30));
    let mut b = run(2, "holdout", perfect(30));
    b.label_ids_hash = "other".to_owned();
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_INCOMPLETE);
}

#[test]
fn gate_on_runs_with_default_labels_as_of_is_comparable_when_hash_matches() {
    let c = run(1, "holdout", perfect(30));
    let b = run(2, "holdout", perfect(30));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_PASS);
}

#[test]
fn gate_with_an_unfinished_noise_run_is_incomplete() {
    let c = run(1, "holdout", perfect(30));
    let b = run(2, "holdout", perfect(30));
    let mut n = run(3, "holdout", perfect(12));
    n.repeat_of_run_id = Some(1);
    n.status = "paused_budget".to_owned();
    let mut req = request(&c, &b);
    req.noise = Some(&n);
    let d = decide(&req);
    assert_eq!((d.verdict, d.exit_code), ("incomplete", EXIT_INCOMPLETE));
}

#[test]
fn gate_with_a_refused_item_is_incomplete() {
    let mut items = perfect(30);
    items[4].outcome = "refused".to_owned();
    items[4].pred_reaction = None;
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(30));
    let d = decide(&request(&c, &b));
    assert_eq!((d.verdict, d.exit_code), ("incomplete", EXIT_INCOMPLETE));
}

#[test]
fn gate_below_precision_floor_fails() {
    let mut items = perfect(30);
    for i in [1, 2, 4] {
        items[i].pred_reaction = Some("corrected".to_owned());
    }
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(30));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_FAIL);
}

#[test]
fn gate_below_recall_floor_fails() {
    let mut items = perfect(30);
    for i in [0, 3, 6, 9, 12] {
        items[i].pred_reaction = Some("accepted".to_owned());
    }
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(30));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_FAIL);
}

#[test]
fn gate_net_loss_within_noise_band_passes() {
    let mut items = perfect(60);
    items[1].pred_reaction = Some("redirected".to_owned());
    items[2].pred_reaction = Some("redirected".to_owned());
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(60));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_PASS);
}

#[test]
fn gate_net_loss_beyond_noise_band_fails() {
    let mut items = perfect(60);
    for i in [1, 2, 4, 5] {
        items[i].pred_reaction = Some("redirected".to_owned());
    }
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(60));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_FAIL);
}

#[test]
fn gate_missed_control_fails() {
    let mut items = perfect(30);
    items[1].control = true;
    items[1].pred_reaction = Some("unclear".to_owned());
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(30));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_FAIL);
}

#[test]
fn gate_no_response_item_judged_otherwise_fails() {
    let mut items = perfect(30);
    items[1].settled_without_reply = true;
    let c = run(1, "holdout", items);
    let b = run(2, "holdout", perfect(30));
    assert_eq!(decide(&request(&c, &b)).exit_code, EXIT_FAIL);
}

#[test]
fn gate_on_dev_split_never_reports_floors_as_met() {
    let c = run(1, "dev", perfect(30));
    let b = run(2, "dev", perfect(30));
    let d = decide(&request(&c, &b));
    assert_eq!(d.exit_code, EXIT_FAIL);
    assert_eq!(d.report["floors_met"], false);
}

#[test]
fn fourth_gate_on_same_holdout_batch_without_reason_is_refused() {
    let c = run(1, "holdout", perfect(30));
    let b = run(2, "holdout", perfect(30));
    let mut req = request(&c, &b);
    req.prior_holdout_gates = 3;
    let d = decide(&req);
    assert_eq!(d.exit_code, EXIT_REFUSED);
    assert!(!d.record);
    req.override_reason = Some("operator asked for a fourth candidate");
    assert_eq!(decide(&req).exit_code, EXIT_PASS);
}

#[test]
fn e_slice_rule_on_holdout_split_is_refused() {
    let c = run(1, "holdout", perfect(30));
    let b = run(2, "holdout", perfect(30));
    let mut req = request(&c, &b);
    req.rule = GateRule::ESlice;
    assert_eq!(decide(&req).exit_code, EXIT_REFUSED);
}

#[test]
fn e_slice_gate_does_not_count_toward_holdout_reuse() {
    let c = run(1, "dev", perfect(30));
    let b = run(2, "dev", perfect(30));
    let mut req = request(&c, &b);
    req.rule = GateRule::ESlice;
    req.prior_holdout_gates = 10;
    assert_ne!(decide(&req).exit_code, EXIT_REFUSED);
}

#[derive(Deserialize)]
struct ERow {
    variant: String,
    turn_id: i64,
    reaction: String,
    delivery: Option<String>,
}

#[derive(Deserialize)]
struct Gold {
    reaction: String,
    delivery: Option<String>,
}

#[derive(Deserialize)]
struct EFixture {
    rows: Vec<ERow>,
    gold: HashMap<String, Gold>,
    expected: HashMap<String, String>,
}

fn e_run(id: i64, variant: &str, fixture: &EFixture) -> GateRun {
    let items = fixture
        .rows
        .iter()
        .filter(|r| r.variant == variant)
        .filter_map(|r| {
            let gold = fixture.gold.get(&r.turn_id.to_string())?;
            Some(GateItem {
                turn_id: r.turn_id,
                outcome: "judged".to_owned(),
                gold_reaction: gold.reaction.clone(),
                gold_delivery: gold.delivery.clone(),
                pred_reaction: Some(r.reaction.clone()),
                pred_delivery: r.delivery.clone(),
                inclusion_prob: None,
                control: false,
                settled_without_reply: false,
            })
        })
        .collect();
    let mut r = run(id, "dev", items);
    if variant.starts_with("VT") {
        r.slices = vec!["turnfacts".to_owned()];
    }
    r
}

#[test]
fn e_slice_rule_reproduces_e_score_on_the_p0_fixture() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/eval/e_score_fixture.json");
    let fixture: EFixture = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let base = e_run(1, "V0_haiku", &fixture);
    let mut noise = e_run(2, "V0r_haiku", &fixture);
    noise.repeat_of_run_id = Some(1);
    for (i, (variant, expected)) in fixture.expected.iter().enumerate() {
        let cand = e_run(10 + i as i64, variant, &fixture);
        let req = GateRequest {
            rule: GateRule::ESlice,
            candidate: &cand,
            baseline: &base,
            noise: Some(&noise),
            prior_holdout_gates: 0,
            override_reason: None,
            floors: Floors::default(),
        };
        let d = decide(&req);
        let go = expected.starts_with("GO");
        assert_eq!(
            d.exit_code == EXIT_PASS,
            go,
            "{variant}: expected {expected}, got {:?}",
            d.report
        );
    }
    Ok(())
}
