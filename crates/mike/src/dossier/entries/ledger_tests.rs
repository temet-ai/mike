use chrono::{Duration, Utc};

use super::ledger::plan_builder_rows;
use crate::store::persistence::ask_ledger_backfill::LedgerCandidateRow;

fn candidate(turn_id: i64, session_id: i64, prompt: &str, has_earlier: bool) -> LedgerCandidateRow {
    LedgerCandidateRow {
        turn_id,
        session_id,
        start_request_id: turn_id * 10,
        user_prompt: prompt.to_owned(),
        created_at: Utc::now() + Duration::seconds(turn_id),
        has_earlier,
    }
}

#[test]
fn first_candidate_of_a_fresh_session_is_initial_and_later_ones_are_not() {
    let rows = plan_builder_rows(&[
        candidate(1, 7, "build the importer", false),
        candidate(2, 7, "and add docs", false),
    ]);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, "initial");
    assert_ne!(rows[1].kind, "initial");
    assert_eq!(rows[0].request_id, 10);
    assert_eq!(rows[0].operator_turn_id, 1);
}

#[test]
fn a_session_with_earlier_ledger_rows_never_gets_an_initial_row() {
    let rows = plan_builder_rows(&[candidate(3, 8, "build the importer", true)]);
    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0].kind, "initial");
}

#[test]
fn empty_prompts_get_no_row() {
    assert!(plan_builder_rows(&[candidate(4, 9, "   ", false)]).is_empty());
}
