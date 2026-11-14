use chrono::{TimeZone, Utc};

use super::*;

fn row(id: i64, status: &str) -> ProposalRow {
    let at = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    ProposalRow {
        id,
        uuid: uuid::Uuid::nil(),
        source: "repeated_directive".to_owned(),
        scope: "global".to_owned(),
        project_id: None,
        class: "tests".to_owned(),
        polarity: "do".to_owned(),
        status: status.to_owned(),
        proposed_kind: "convention".to_owned(),
        proposed_text: format!("directive {id}"),
        evidence_count: 4,
        session_count: 3,
        first_evidence_at: at,
        last_evidence_at: at,
        window_days: 30,
        rollup_id: 1,
        proposer_version: "v1".to_owned(),
        created_at: at,
        decided_at: None,
        decision_note: None,
        approved_text: None,
        superseded_by: None,
    }
}

#[test]
fn order_by_suggestion_puts_likely_approvals_first_and_decided_rows_last() {
    let mut rows = vec![
        row(1, "open"),
        row(2, "approved"),
        row(3, "open"),
        row(4, "open"),
    ];
    let suggested = BTreeMap::from([(3, 0.9), (1, 0.2)]);
    order_by_suggestion(&mut rows, &suggested);
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![3, 1, 4, 2]
    );
}

#[test]
fn candidate_describes_the_directive_for_jev() {
    let built = candidate(&row(8, "open"));
    assert_eq!(built.key, "directive:8");
    assert_eq!(built.question_key, "directive_8");
    assert_eq!(built.item["directive_kind"], "convention");
    assert_eq!(built.item["session_count"], 3);
}

#[test]
fn cached_is_empty_without_a_background_guess() {
    assert!(cached(&[row(900_002, "open")]).is_empty());
}
