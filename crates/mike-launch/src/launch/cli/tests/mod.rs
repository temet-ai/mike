#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

pub mod doctor_tests;
pub mod kill_tests;
pub mod list_tests;
pub mod open_prompt_tests;
pub mod open_tests;
pub mod prompt_turn_tests;
pub mod proxy_ledger_tests;
pub mod queue_brief_tests;
pub mod queue_tests;

pub use super::test_kit::{fake, project_dir, runner, toks};

pub fn json(line: &str) -> serde_json::Value {
    serde_json::from_str(line).unwrap()
}

pub fn assert_key_order(line: &str, expected: &[&str]) {
    let positions: Vec<usize> = expected
        .iter()
        .map(|k| {
            line.find(&format!("\"{k}\":"))
                .unwrap_or_else(|| panic!("key {k} missing from {line}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "key order in {line}"
    );
    let value = json(line);
    assert_eq!(value.as_object().unwrap().len(), expected.len(), "{line}");
}
