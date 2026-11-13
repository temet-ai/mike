use serde_json::{Value, json};

pub const ARTIFACT: &str = "plans/judgement-context/p0/out/c.txt";
pub const WINDOW: &str = "2026-07-27 to 2026-09-23 17:22Z";
pub const FLOOR_PRECISION: f64 = 0.50;
pub const FLOOR_RECALL: f64 = 0.096;
pub const FIRED_CENSUS_TURNS: i64 = 150;
pub const FIRED_CORRECTED: i64 = 75;
pub const RELEVANT_CORRECTIONS: i64 = 197;
pub const RELEVANT_FIRED: i64 = 19;
pub const BASE_RATE: f64 = 0.409;
pub const FIRED_BEFORE_2026_09_21: i64 = 144;

pub fn baseline_json() -> Value {
    json!({
        "artifact": ARTIFACT,
        "window": WINDOW,
        "precision": {"k": FIRED_CORRECTED, "n": FIRED_CENSUS_TURNS, "value": FLOOR_PRECISION},
        "recall": {"k": RELEVANT_FIRED, "n": RELEVANT_CORRECTIONS, "value": FLOOR_RECALL},
        "base_rate": BASE_RATE,
        "fired_census_turns_before_2026_09_21": FIRED_BEFORE_2026_09_21,
        "configuration": "uncapped verifier input (before JC21)",
    })
}
