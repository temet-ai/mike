use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};

use super::score::bootstrap::bootstrap_ci;
use super::score::metrics::{REACTION_CLASSES, WeightedItem, class_counts, weighted_rates};

pub const MAX_HOLDOUT_GATES: i64 = 3;
pub const EXIT_PASS: i32 = 0;
pub const EXIT_FAIL: i32 = 1;
pub const EXIT_INCOMPLETE: i32 = 2;
pub const EXIT_REFUSED: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Floors {
    pub precision: f64,
    pub recall: f64,
}

impl Default for Floors {
    fn default() -> Self {
        Self {
            precision: 0.85,
            recall: 0.65,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateRule {
    Promotion,
    ESlice,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateItem {
    pub turn_id: i64,
    pub outcome: String,
    pub gold_reaction: String,
    pub gold_delivery: Option<String>,
    pub pred_reaction: Option<String>,
    pub pred_delivery: Option<String>,
    pub inclusion_prob: Option<f64>,
    pub control: bool,
    pub settled_without_reply: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateRun {
    pub run_id: i64,
    pub status: String,
    pub label_ids_hash: String,
    pub split: String,
    pub holdout_batch: Option<String>,
    pub repeat_of_run_id: Option<i64>,
    pub r3: bool,
    pub slices: Vec<String>,
    pub items: Vec<GateItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateDecision {
    pub verdict: &'static str,
    pub exit_code: i32,
    pub record: bool,
    pub floors: Value,
    pub report: Value,
}

pub struct GateRequest<'a> {
    pub rule: GateRule,
    pub candidate: &'a GateRun,
    pub baseline: &'a GateRun,
    pub noise: Option<&'a GateRun>,
    pub prior_holdout_gates: i64,
    pub override_reason: Option<&'a str>,
    pub floors: Floors,
}

fn decision(verdict: &'static str, exit_code: i32, floors: Value, report: Value) -> GateDecision {
    GateDecision {
        verdict,
        exit_code,
        record: exit_code != EXIT_REFUSED,
        floors,
        report,
    }
}

fn refused(reason: &str, floors: Value) -> GateDecision {
    decision(
        "refused",
        EXIT_REFUSED,
        floors,
        json!({ "refused": reason }),
    )
}

fn comparable(req: &GateRequest<'_>) -> Option<String> {
    let (c, b) = (req.candidate, req.baseline);
    if c.status != "complete" || b.status != "complete" {
        return Some(format!(
            "runs must be complete (candidate {}, baseline {})",
            c.status, b.status
        ));
    }
    if c.label_ids_hash != b.label_ids_hash
        || c.split != b.split
        || c.holdout_batch != b.holdout_batch
    {
        return Some("candidate and baseline did not judge the same label set".to_owned());
    }
    if let Some(n) = req.noise {
        if n.status != "complete" {
            return Some(format!(
                "the noise run must be complete (noise {})",
                n.status
            ));
        }
        let repeats = n.repeat_of_run_id == Some(c.run_id) || n.repeat_of_run_id == Some(b.run_id);
        if !repeats || n.label_ids_hash != c.label_ids_hash {
            return Some(
                "the noise run must repeat the candidate or baseline on the same labels".to_owned(),
            );
        }
    }
    None
}

fn has_missing(run: &GateRun) -> bool {
    run.items
        .iter()
        .any(|i| i.outcome == "refused" || i.outcome == "failed")
}

fn correct_on(item: &GateItem, class: &str, r3: bool) -> Option<bool> {
    let pred = item.pred_reaction.as_deref()?;
    if r3 {
        return Some((pred == "corrected") == (item.gold_reaction == "corrected"));
    }
    Some((pred == class) == (item.gold_reaction == class))
}

fn flips(a: &GateRun, b: &GateRun, class: &str, r3: bool) -> (i64, i64) {
    let other: HashMap<i64, &GateItem> = b.items.iter().map(|i| (i.turn_id, i)).collect();
    let mut a_right_b_wrong = 0;
    let mut a_wrong_b_right = 0;
    for item in &a.items {
        let Some(peer) = other.get(&item.turn_id) else {
            continue;
        };
        match (correct_on(item, class, r3), correct_on(peer, class, r3)) {
            (Some(true), Some(false)) => a_right_b_wrong += 1,
            (Some(false), Some(true)) => a_wrong_b_right += 1,
            _ => {}
        }
    }
    (a_right_b_wrong, a_wrong_b_right)
}

fn noise_flips(noise: Option<&GateRun>, req: &GateRequest<'_>, class: &str, r3: bool) -> i64 {
    let Some(noise) = noise else {
        return 0;
    };
    let repeated = if noise.repeat_of_run_id == Some(req.candidate.run_id) {
        req.candidate
    } else {
        req.baseline
    };
    let (x, y) = flips(repeated, noise, class, r3);
    x + y
}

fn corrected_rates(run: &GateRun) -> (Option<f64>, Option<f64>, bool) {
    let weighted_batch = run.items.iter().any(|i| i.inclusion_prob.is_some());
    let judged: Vec<&GateItem> = run
        .items
        .iter()
        .filter(|i| i.pred_reaction.is_some())
        .collect();
    if weighted_batch {
        let items: Vec<WeightedItem> = judged
            .iter()
            .map(|i| WeightedItem {
                predicted: i.pred_reaction.as_deref() == Some("corrected"),
                positive: i.gold_reaction == "corrected",
                inclusion_prob: i.inclusion_prob,
            })
            .collect();
        let w = weighted_rates(&items);
        return (w.precision, w.recall, true);
    }
    let pairs: Vec<(&str, &str)> = judged
        .iter()
        .filter_map(|i| Some((i.gold_reaction.as_str(), i.pred_reaction.as_deref()?)))
        .collect();
    let c = class_counts(&pairs, "corrected");
    (c.precision, c.recall, false)
}

fn controls_hold(run: &GateRun) -> Vec<i64> {
    run.items
        .iter()
        .filter(|i| {
            let wrong_control =
                i.control && i.pred_reaction.as_deref() != Some(i.gold_reaction.as_str());
            let wrong_settled =
                i.settled_without_reply && i.pred_reaction.as_deref() != Some("no_response");
            wrong_control || wrong_settled
        })
        .map(|i| i.turn_id)
        .collect()
}

fn regression_report(req: &GateRequest<'_>) -> (bool, Value) {
    let r3 = req.baseline.r3 || req.candidate.r3;
    let classes: Vec<&str> = if r3 {
        vec!["corrected"]
    } else {
        REACTION_CLASSES.to_vec()
    };
    let mut ok = true;
    let mut table = serde_json::Map::new();
    for class in classes {
        let (base_right_cand_wrong, base_wrong_cand_right) =
            flips(req.baseline, req.candidate, class, r3);
        let net_loss = base_right_cand_wrong - base_wrong_cand_right;
        let allowance = noise_flips(req.noise, req, class, r3).max(2);
        ok &= net_loss <= allowance;
        table.insert(
            class.to_owned(),
            json!({ "lost": base_right_cand_wrong, "gained": base_wrong_cand_right, "net_loss": net_loss, "allowance": allowance }),
        );
    }
    (ok, Value::Object(table))
}

fn promotion(req: &GateRequest<'_>, floors: Value) -> GateDecision {
    let holdout = req.candidate.split == "holdout";
    let (precision, recall, weighted) = corrected_rates(req.candidate);
    let floors_met = holdout
        && precision.is_some_and(|p| p >= req.floors.precision)
        && recall.is_some_and(|r| r >= req.floors.recall);
    let (regression_ok, regression) = regression_report(req);
    let missed_controls = controls_hold(req.candidate);
    let report = json!({
        "corrected": { "precision": precision, "recall": recall, "weighted": weighted },
        "floors_applicable": holdout,
        "floors_met": floors_met,
        "regression": regression,
        "missed_controls": missed_controls,
        "override_reason": req.override_reason,
    });
    if floors_met && regression_ok && missed_controls.is_empty() {
        decision("pass", EXIT_PASS, floors, report)
    } else {
        decision("fail", EXIT_FAIL, floors, report)
    }
}

pub fn decide(req: &GateRequest<'_>) -> GateDecision {
    let floors = match req.rule {
        GateRule::Promotion => {
            json!({ "rule": "promotion", "precision": req.floors.precision, "recall": req.floors.recall })
        }
        GateRule::ESlice => json!({ "rule": "e-slice" }),
    };
    let holdout = req.candidate.split == "holdout";
    if req.rule == GateRule::ESlice && holdout {
        return refused("the e-slice rule runs on the dev split only", floors);
    }
    if req.rule == GateRule::Promotion
        && holdout
        && req.prior_holdout_gates >= MAX_HOLDOUT_GATES
        && req.override_reason.is_none()
    {
        return refused(
            "this holdout batch has already judged three candidates; pass --override-reason",
            floors,
        );
    }
    if let Some(reason) = comparable(req) {
        return decision(
            "incomplete",
            EXIT_INCOMPLETE,
            floors,
            json!({ "incomplete": reason }),
        );
    }
    if has_missing(req.candidate) || has_missing(req.baseline) {
        return decision(
            "incomplete",
            EXIT_INCOMPLETE,
            floors,
            json!({ "incomplete": "a run has refused or failed items" }),
        );
    }
    match req.rule {
        GateRule::Promotion => promotion(req, floors),
        GateRule::ESlice => e_slice(req, floors),
    }
}

type Triple<'a> = (&'a str, &'a str, &'a str);

fn f1_of_column(items: &[&Triple<'_>], candidate: bool) -> Option<f64> {
    let pairs: Vec<(&str, &str)> = items
        .iter()
        .map(|(g, b, c)| (*g, if candidate { *c } else { *b }))
        .collect();
    class_counts(&pairs, "corrected").f1
}

fn precision_of_column(items: &[&Triple<'_>], candidate: bool) -> Option<f64> {
    let pairs: Vec<(&str, &str)> = items
        .iter()
        .map(|(g, b, c)| (*g, if candidate { *c } else { *b }))
        .collect();
    class_counts(&pairs, "corrected").precision
}

fn delta<F>(items: &[&Triple<'_>], f: F) -> Option<f64>
where
    F: Fn(&[&Triple<'_>], bool) -> Option<f64>,
{
    Some(f(items, true)? - f(items, false)?)
}

fn triples<'a>(base: &'a GateRun, cand: &'a GateRun, delivery: bool) -> Vec<Triple<'a>> {
    let by_turn: HashMap<i64, &GateItem> = cand.items.iter().map(|i| (i.turn_id, i)).collect();
    base.items
        .iter()
        .filter_map(|b| {
            let c = by_turn.get(&b.turn_id)?;
            if delivery {
                Some((
                    b.gold_delivery.as_deref()?,
                    b.pred_delivery.as_deref()?,
                    c.pred_delivery.as_deref()?,
                ))
            } else {
                Some((
                    b.gold_reaction.as_str(),
                    b.pred_reaction.as_deref()?,
                    c.pred_reaction.as_deref()?,
                ))
            }
        })
        .collect()
}

fn net_flips(items: &[Triple<'_>], delivery: bool) -> i64 {
    let ok = |g: &str, p: &str| {
        if delivery {
            g == p
        } else {
            (p == "corrected") == (g == "corrected")
        }
    };
    let w2r = items
        .iter()
        .filter(|(g, b, c)| !ok(g, b) && ok(g, c))
        .count() as i64;
    let r2w = items
        .iter()
        .filter(|(g, b, c)| ok(g, b) && !ok(g, c))
        .count() as i64;
    w2r - r2w
}

fn accuracy_of_column(items: &[&Triple<'_>], candidate: bool) -> Option<f64> {
    if items.is_empty() {
        return None;
    }
    let right = items
        .iter()
        .filter(|(g, b, c)| *g == if candidate { *c } else { *b })
        .count();
    Some(right as f64 / items.len() as f64)
}

pub fn e_slice_axes(base: &GateRun, cand: &GateRun, noise: Option<&GateRun>, seed: u64) -> Value {
    let reaction = triples(base, cand, false);
    let delivery = triples(base, cand, true);
    let drift_r = noise.map_or(0, |n| net_flips(&triples(base, n, false), false).abs());
    let drift_d = noise.map_or(0, |n| net_flips(&triples(base, n, true), true).abs());
    let f1_ci = bootstrap_ci(&reaction, seed, |s| delta(s, f1_of_column));
    let p_ci = bootstrap_ci(&reaction, seed, |s| delta(s, precision_of_column));
    let acc_ci = bootstrap_ci(&delivery, seed, |s| delta(s, accuracy_of_column));
    let r_net = net_flips(&reaction, false);
    let d_net = net_flips(&delivery, true);
    let r_go = f1_ci.is_some_and(|(lo, _)| lo > 0.0)
        && p_ci.is_some_and(|(lo, _)| lo > -0.05)
        && r_net >= (2 * drift_r).max(3);
    let d_go = acc_ci.is_some_and(|(lo, _)| lo > 0.0) && d_net >= (2 * drift_d).max(3);
    json!({
        "reaction": { "n": reaction.len(), "delta_f1_ci": f1_ci, "delta_precision_ci": p_ci, "net": r_net, "drift": drift_r, "go": r_go },
        "delivery": { "n": delivery.len(), "delta_acc_ci": acc_ci, "net": d_net, "drift": drift_d, "go": d_go },
    })
}

fn e_slice(req: &GateRequest<'_>, floors: Value) -> GateDecision {
    let axes = e_slice_axes(
        req.baseline,
        req.candidate,
        req.noise,
        u64::try_from(req.candidate.run_id).unwrap_or(0),
    );
    let turnfacts = req.candidate.slices.iter().any(|s| s == "turnfacts");
    let reaction_go = axes["reaction"]["go"].as_bool().unwrap_or(false);
    let delivery_go = turnfacts && axes["delivery"]["go"].as_bool().unwrap_or(false);
    let axis = if reaction_go {
        "reaction"
    } else if delivery_go {
        "delivery"
    } else {
        "none"
    };
    let report = json!({ "axes": axes, "go_axis": axis, "slices": req.candidate.slices });
    if reaction_go || delivery_go {
        decision("pass", EXIT_PASS, floors, report)
    } else {
        decision("fail", EXIT_FAIL, floors, report)
    }
}
