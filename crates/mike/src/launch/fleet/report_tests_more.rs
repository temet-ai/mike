use serde_json::json;

use super::{WORKER, at, def, held, id, report_request};
use crate::launch::fleet::report::{
    CHECK_NOT_YET, CommitFacts, ReportRequest, VerdictRequest, acceptance_mail, check_report,
    report_body,
};

#[test]
fn a_summary_sent_by_the_seat_is_read_from_the_request() -> anyhow::Result<()> {
    let req: ReportRequest = serde_json::from_value(json!({
        "task": "P1", "slice": "s", "summary": "the commands and their verdicts"
    }))?;
    assert_eq!(report_body(&req), "the commands and their verdicts");
    Ok(())
}

#[test]
fn criteria_accept_arrays_or_comma_separated_text() -> anyhow::Result<()> {
    for input in [json!(["C3", "C5"]), json!("C3, C5"), json!(" C3 , C5, C3 ")] {
        let report: ReportRequest = serde_json::from_value(json!({"criteria": input}))?;
        let verdict: VerdictRequest = serde_json::from_value(json!({"criteria": input}))?;
        assert_eq!(report.criteria, vec!["C3", "C5"]);
        assert_eq!(verdict.criteria, report.criteria);
    }
    let report: ReportRequest = serde_json::from_value(json!({"criteria": "C3(e)"}))?;
    assert_eq!(report.criteria, vec!["C3(e)"]);
    assert!(
        check_report(
            &def(),
            &held(),
            &ReportRequest {
                criteria: report.criteria,
                ..report_request(WORKER, "P1")
            }
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn criteria_default_only_when_omitted_or_an_empty_array() -> anyhow::Result<()> {
    for input in [json!({}), json!({"criteria": []})] {
        let report: ReportRequest = serde_json::from_value(input)?;
        assert!(report.criteria.is_empty());
    }
    for input in [
        json!(""),
        json!("C3,"),
        json!([" "]),
        json!(3),
        json!(null),
        json!([3]),
    ] {
        assert!(serde_json::from_value::<ReportRequest>(json!({"criteria": input})).is_err());
        assert!(serde_json::from_value::<VerdictRequest>(json!({"criteria": input})).is_err());
    }
    Ok(())
}

#[test]
fn acceptance_receipt_mail_is_bounded_and_points_to_durable_note() {
    let row = crate::store::persistence::fleet::TreeResultRow {
        use_id: 9,
        seat: Some("db-dev-P1".into()),
        session_uuid: Some(id(WORKER)),
        command: Some(format!(
            "cargo test {}",
            "large-private-test-name".repeat(1_000)
        )),
        started_at: Some(at(1)),
        finished_at: Some(at(2)),
        is_error: Some(false),
        content: None,
    };
    let mut proof = crate::launch::fleet::report_proof::from_rows(&[row], Some(&[]), &[]);
    proof.limit_reached = true;
    proof.unavailable = Some("history unavailable".repeat(1_000));
    let facts = CommitFacts {
        commit: None,
        check: CHECK_NOT_YET.into(),
        checked_after_last_change: None,
        brief_version: None,
        check_command: "cargo check".into(),
        proof,
    };
    let text = acceptance_mail(
        "db-dev-P1",
        &report_request(WORKER, "P1"),
        &[],
        &facts,
        id(90),
    );
    assert!(text.len() < 2_000);
    assert!(text.contains("Recorded test/lint receipts: 1"));
    assert!(text.contains("query limit reached: true"));
    assert!(text.contains("lookup: unavailable; see note"));
    assert!(text.contains("data.proof in the report note"));
    assert!(text.contains(&id(90).to_string()));
    assert!(!text.contains("large-private-test-name"));
    assert_eq!(facts.proof.receipts.len(), 1);
}
