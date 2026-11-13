use super::check::{CheckReport, WITHDRAWN};
use super::definition::CheckDefinition;

pub fn blank(seat: &str, check: &CheckDefinition, how: &str) -> CheckReport {
    CheckReport {
        receipt_id: None,
        command_receipt: false,
        baseline: false,
        seat: seat.to_string(),
        command: check.command.clone(),
        state: String::new(),
        commit: String::new(),
        how: how.to_string(),
        passed: false,
        exit_code: None,
        seconds: 0,
        askers: Vec::new(),
        mine: Vec::new(),
        others: Vec::new(),
        unowned: Vec::new(),
        owned: Vec::new(),
        output_tail: String::new(),
        refused: None,
        started_at: None,
    }
}

pub fn withdrawn(seat: &str, check: &CheckDefinition, receipt: i64) -> CheckReport {
    match super::managed_queue::withdraw(receipt, seat) {
        Ok(said) => CheckReport {
            receipt_id: Some(receipt),
            output_tail: said,
            ..blank(seat, check, WITHDRAWN)
        },
        Err(why) => refused(seat, check, why),
    }
}

pub fn refused(seat: &str, check: &CheckDefinition, why: String) -> CheckReport {
    CheckReport {
        refused: Some(why),
        ..blank(seat, check, "refused")
    }
}
