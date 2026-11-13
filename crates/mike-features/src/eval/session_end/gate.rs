use serde::Serialize;
use serde_json::{Value, json};

use super::score::{EndItem, Variant, frustration_rates, pairs, rule_precision, variant_kappa};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SessionEndFloors {
    pub min_kappa: f64,
    pub max_kappa: f64,
    pub min_frustration_precision: f64,
    pub min_frustration_recall: f64,
    pub min_fleet_seat_precision: f64,
    pub min_done_precision: f64,
    pub max_done_frustration_among_fires: f64,
}

impl Default for SessionEndFloors {
    fn default() -> Self {
        Self {
            min_kappa: 0.50,
            max_kappa: 0.75,
            min_frustration_precision: 0.55,
            min_frustration_recall: 0.65,
            min_fleet_seat_precision: 0.95,
            min_done_precision: 0.80,
            max_done_frustration_among_fires: 0.02,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionEndGateDecision {
    pub verdict: &'static str,
    pub variant: &'static str,
    pub failures: Vec<String>,
    pub floors: Value,
}

pub struct SessionEndGateRequest<'a> {
    pub candidate: &'a [EndItem],
    pub noise: Option<&'a [EndItem]>,
    pub default_rules: &'a [&'a str],
    pub floors: SessionEndFloors,
}

fn below(value: Option<f64>, floor: f64) -> bool {
    value.is_none_or(|v| v < floor)
}

fn noise_delta(req: &SessionEndGateRequest<'_>, variant: Variant) -> f64 {
    let Some(noise) = req.noise else {
        return 0.0;
    };
    match (
        variant_kappa(req.candidate, variant),
        variant_kappa(noise, variant),
    ) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => 0.0,
    }
}

fn rule_failures(req: &SessionEndGateRequest<'_>, failures: &mut Vec<String>) {
    let f = req.floors;
    let model_kappa = variant_kappa(req.candidate, Variant::ModelOnly);
    let delta = noise_delta(req, Variant::ModelOnly);
    for rule in req.default_rules {
        let variant = Variant::for_rules(&[rule]);
        let kappa = variant_kappa(req.candidate, variant);
        if let (Some(k), Some(m)) = (kappa, model_kappa)
            && k < m - delta
        {
            failures.push(format!(
                "F4: {rule} kappa {k:.3} is below model-only {m:.3} minus noise {delta:.3}"
            ));
        }
        let (precision, frustrated) = rule_precision(req.candidate, rule);
        match *rule {
            "fleet-seat-v1" if below(precision.value, f.min_fleet_seat_precision) => {
                failures.push(format!(
                    "F4: fleet-seat-v1 precision {:?} is below {}",
                    precision.value, f.min_fleet_seat_precision
                ))
            }
            "done-v1" if below(precision.value, f.min_done_precision) => failures.push(format!(
                "F4: done-v1 precision {:?} is below {}",
                precision.value, f.min_done_precision
            )),
            "done-v1"
                if frustrated
                    .value
                    .is_some_and(|v| v > f.max_done_frustration_among_fires) =>
            {
                failures.push(format!(
                    "F4: done-v1 frustration among fires {:?} is above {}",
                    frustrated.value, f.max_done_frustration_among_fires
                ))
            }
            _ => {}
        }
    }
}

pub fn decide_session_end(req: &SessionEndGateRequest<'_>) -> SessionEndGateDecision {
    let f = req.floors;
    let variant = Variant::for_rules(req.default_rules);
    let kappa = variant_kappa(req.candidate, variant);
    let (precision, recall) = frustration_rates(&pairs(req.candidate, variant));
    let mut failures = Vec::new();
    if below(kappa, f.min_kappa) {
        failures.push(format!("F1: kappa {kappa:?} is below {}", f.min_kappa));
    }
    if below(precision.value, f.min_frustration_precision)
        || below(recall.value, f.min_frustration_recall)
    {
        failures.push(format!(
            "F2: frustration precision {:?} / recall {:?} is below {} / {}",
            precision.value, recall.value, f.min_frustration_precision, f.min_frustration_recall
        ));
    }
    rule_failures(req, &mut failures);
    let leakage = kappa.is_some_and(|k| k > f.max_kappa);
    let verdict = match (failures.is_empty(), leakage) {
        (false, _) => "fail",
        (true, true) => "incomplete",
        (true, false) => "pass",
    };
    if leakage {
        failures.push(format!(
            "F3: kappa {kappa:?} is above {}; a leakage review is needed",
            f.max_kappa
        ));
    }
    SessionEndGateDecision {
        verdict,
        variant: variant.key(),
        failures,
        floors: json!(f),
    }
}

pub fn exit_code(verdict: &str) -> i32 {
    match verdict {
        "pass" => 0,
        "incomplete" => 2,
        _ => 1,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn item(gold: &str, llm: &str, rule: Option<&str>) -> EndItem {
        EndItem {
            gold: gold.to_owned(),
            llm: Some(llm.to_owned()),
            rule_fired: rule.map(str::to_owned),
        }
    }

    fn balanced(correct: usize, wrong: usize) -> Vec<EndItem> {
        let mut items = Vec::new();
        for label in [
            "done",
            "closed_in_frustration",
            "cut_off_neutral",
            "fleet_seat",
            "unclear",
        ] {
            items.extend((0..correct).map(|_| item(label, label, None)));
            items.extend((0..wrong).map(|_| item(label, "unclear", None)));
        }
        items
    }

    #[test]
    fn decide_session_end_fails_f1_below_the_kappa_floor() {
        let items = balanced(1, 3);
        let d = decide_session_end(&SessionEndGateRequest {
            candidate: &items,
            noise: None,
            default_rules: &[],
            floors: SessionEndFloors::default(),
        });
        assert_eq!(d.verdict, "fail");
        assert!(d.failures.iter().any(|f| f.starts_with("F1")));
    }

    #[test]
    fn decide_session_end_marks_perfect_agreement_incomplete_for_leakage() {
        let items = balanced(5, 0);
        let d = decide_session_end(&SessionEndGateRequest {
            candidate: &items,
            noise: None,
            default_rules: &[],
            floors: SessionEndFloors::default(),
        });
        assert_eq!(d.verdict, "incomplete");
        assert_eq!(exit_code(d.verdict), 2);
    }

    #[test]
    fn decide_session_end_passes_inside_the_band() {
        let mut items = balanced(6, 3);
        items.retain(|i| {
            !(i.gold == "closed_in_frustration" && i.llm.as_deref() == Some("unclear"))
        });
        let d = decide_session_end(&SessionEndGateRequest {
            candidate: &items,
            noise: None,
            default_rules: &[],
            floors: SessionEndFloors::default(),
        });
        assert_eq!(d.verdict, "pass", "{:?}", d.failures);
    }

    #[test]
    fn decide_session_end_f4_rejects_a_done_rule_that_fires_on_frustration() {
        let mut items = balanced(8, 2);
        items.push(item(
            "closed_in_frustration",
            "closed_in_frustration",
            Some("done-v1"),
        ));
        items.push(item("done", "done", Some("done-v1")));
        let d = decide_session_end(&SessionEndGateRequest {
            candidate: &items,
            noise: None,
            default_rules: &["done-v1"],
            floors: SessionEndFloors::default(),
        });
        assert!(
            d.failures.iter().any(|f| f.contains("done-v1")),
            "{:?}",
            d.failures
        );
    }
}
