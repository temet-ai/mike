use chrono::{DateTime, Utc};

use super::Question;
use crate::cross_session::CrossSessionSnapshot;
use crate::cross_session::calibration_block::{CalibrationBlock, calibration_block};

pub const CALIBRATION_PARAGRAPH: &str = include_str!("prompt/calibration.md");

pub fn block_for(
    snapshot: &CrossSessionSnapshot,
    open: &[(Question, f64)],
    project_id: Option<i64>,
    as_of: DateTime<Utc>,
) -> Option<CalibrationBlock> {
    let questions: Vec<(&str, f64)> = open.iter().map(|(q, t)| (q.code(), *t)).collect();
    calibration_block(snapshot, &questions, project_id, as_of)
}

pub fn system_prompt(base: &str, block: Option<&CalibrationBlock>) -> String {
    match block {
        Some(_) => format!("{base}{CALIBRATION_PARAGRAPH}"),
        None => base.to_owned(),
    }
}

pub fn with_block(input: &str, block: Option<&CalibrationBlock>) -> String {
    match block {
        Some(b) => format!("{input}## CALIBRATION\n{}\n\n", b.text),
        None => input.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_mode_leaves_the_call_byte_identical() {
        let s = CrossSessionSnapshot::default();
        let block = block_for(&s, &[(Question::Relapse, 0.75)], Some(1), Utc::now());
        assert!(block.is_none());
        assert_eq!(system_prompt("BASE", block.as_ref()), "BASE");
        assert_eq!(with_block("INPUT", block.as_ref()), "INPUT");
    }
}
