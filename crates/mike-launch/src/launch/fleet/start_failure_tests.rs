use chrono::Utc;
use uuid::Uuid;

use super::notice;
use crate::store::persistence::fleet::FleetRow;

const SENDER: Uuid = Uuid::from_u128(0x0c16_da5b_0000_4000_8000_0000_0000_0001);
const FLEET: Uuid = Uuid::from_u128(0x0c16_da5b_0000_4000_8000_0000_0000_0002);

fn row(created_by: Option<Uuid>) -> FleetRow {
    FleetRow {
        id: 1,
        fleet_uuid: FLEET,
        slug: "open-issues".into(),
        project_path: "/repo".into(),
        state: "failed".into(),
        definition: serde_json::json!({}),
        missing: serde_json::json!([]),
        run_id: Some("fleet-open-issues-0c16da5b".into()),
        start_report: None,
        created_by,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        ready_at: Some(Utc::now()),
        started_at: None,
        finished_at: None,
    }
}

#[test]
fn an_unverified_baseline_mails_the_sender_one_line_with_the_reason_and_the_fix()
-> anyhow::Result<()> {
    let problem = "startup gate could not establish a verified baseline; full output is in tree-check result 2185:\nerror[E0425]: cannot find value\n  --> src/lib.rs:3";
    let told = notice(&row(Some(SENDER)), problem).ok_or_else(|| anyhow::anyhow!("not told"))?;
    assert_eq!(told.to, SENDER);
    assert_eq!(told.mail.lines().count(), 1, "{}", told.mail);
    assert!(
        told.mail.starts_with("[mike] Fleet open-issues did not start: startup gate could not establish a verified baseline; full output is in tree-check result 2185."),
        "{}",
        told.mail
    );
    assert!(
        told.mail.contains("Fix: read the tree-check result"),
        "{}",
        told.mail
    );
    assert!(
        told.note_body.contains("cannot find value"),
        "{}",
        told.note_body
    );
    assert_eq!(told.tag, format!("fleet:{FLEET}"));
    Ok(())
}

#[test]
fn a_reason_that_already_says_how_to_retry_gets_no_second_fix() -> anyhow::Result<()> {
    let problem = "the gate `scripts/gate check` fails on the branch's clean commit (tree-check result 9, ledger x), and a red baseline starts only baseline.repair_task, which the definition does not name, so no worker could start. Add a dependency-free task that makes the gate pass, set baseline.repair_task to it, and send ready again";
    let told = notice(&row(Some(SENDER)), problem).ok_or_else(|| anyhow::anyhow!("not told"))?;
    assert!(!told.mail.contains("Fix:"), "{}", told.mail);
    assert!(told.mail.ends_with("send ready again."), "{}", told.mail);
    Ok(())
}

#[test]
fn a_fleet_nobody_readied_has_no_one_to_tell() {
    assert_eq!(notice(&row(None), "the database is not reachable"), None);
}

#[test]
fn any_other_failure_names_the_generic_fix() -> anyhow::Result<()> {
    let told = notice(&row(Some(SENDER)), "fleet startup was cancelled")
        .ok_or_else(|| anyhow::anyhow!("not told"))?;
    assert!(
        told.mail.ends_with(
            "correct what the reason names, then send ready again (define_fleet ready: true)."
        ),
        "{}",
        told.mail
    );
    Ok(())
}
