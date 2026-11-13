use super::*;
use serde_json::json;
use uuid::Uuid;

pub fn entry() -> anyhow::Result<AcceptanceObligation> {
    Ok(serde_json::from_value(json!({
        "note_id": Uuid::from_u128(1), "session_uuid": Uuid::from_u128(2),
        "obligation_id": "Q29", "title": "Stable ids", "body": "",
        "data": {"tasks": ["T1", "T2"], "ruling": {
            "version": 1, "command": "grep -q bad ids.txt", "expected_exit_code": 1
        }},
        "tasks": ["T1", "T2"], "criteria": [], "outcome": "pending", "required": true,
        "issue": null, "at": "2026-10-12T06:00:00Z", "updated_at": "2026-10-12T06:00:00Z"
    }))?)
}

#[test]
fn executable_rulings_require_explicit_version_command_tasks_and_expected_exit()
-> anyhow::Result<()> {
    let entry = entry()?;
    assert!(parse(&entry.data).map_err(anyhow::Error::msg)?.is_some());
    assert!(
        parse(&json!({"text": "run a grep before reporting"}))
            .map_err(anyhow::Error::msg)?
            .is_none()
    );
    for change in [
        json!(null),
        json!({}),
        json!({"version": 1, "command": "test -f ok"}),
        json!({"version": 0, "command": "true", "expected_exit_code": 0}),
        json!({"version": 1, "command": " ", "expected_exit_code": 0}),
        json!({"version": 1, "command": "true", "expected_exit_code": 137}),
        json!({"version": 1, "command": "true", "expected_exit_code": 0, "extra": true}),
    ] {
        let mut data = entry.data.clone();
        data["ruling"] = change;
        assert!(parse(&data).is_err(), "{data}");
    }
    let mut unscoped = entry.data;
    unscoped["tasks"] = json!([]);
    assert!(parse(&unscoped).is_err());
    Ok(())
}

#[test]
fn ruling_receipt_identity_binds_note_version_command_expected_result_and_scope()
-> anyhow::Result<()> {
    let mut entry = entry()?;
    let mut rule = executable(&entry).map_err(anyhow::Error::msg)?;
    let original = ledger("run", &entry, &rule);
    rule.version += 1;
    assert_ne!(original, ledger("run", &entry, &rule));
    rule.version -= 1;
    rule.command.push_str("; true");
    assert_ne!(original, ledger("run", &entry, &rule));
    rule = executable(&entry).map_err(anyhow::Error::msg)?;
    rule.expected_exit_code = 0;
    assert_ne!(original, ledger("run", &entry, &rule));
    rule = executable(&entry).map_err(anyhow::Error::msg)?;
    entry.tasks.pop();
    assert_ne!(original, ledger("run", &entry, &rule));
    entry.tasks.push("T2".into());
    entry.outcome = "accepted".into();
    assert_eq!(original, ledger("run", &entry, &rule));
    entry.note_id = Uuid::from_u128(3);
    assert_ne!(original, ledger("run", &entry, &rule));
    Ok(())
}

#[test]
fn unauthorized_or_malformed_rulings_never_become_executable() -> anyhow::Result<()> {
    let mut entry = entry()?;
    entry.issue = Some("the author is not the fleet's acceptance owner".into());
    assert!(executable(&entry).is_err());
    assert_eq!(current_rules(&[entry], "T3").len(), 0);
    Ok(())
}

#[tokio::test]
async fn executable_contract_compares_real_exit_code_and_keeps_full_output() -> anyhow::Result<()> {
    let mut rule = Ruling {
        version: 1,
        command: "printf 'proof output'; exit 1".into(),
        expected_exit_code: 1,
    };
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command(&rule))
        .output()
        .await?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("proof output"));
    rule.expected_exit_code = 0;
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command(&rule))
        .output()
        .await?;
    assert!(!output.status.success());
    Ok(())
}

#[test]
fn accepted_ruling_and_final_pass_without_machine_receipts_stay_unaccepted() -> anyhow::Result<()> {
    let mut entry = entry()?;
    entry.outcome = "accepted".into();
    entry.data["observation"] = json!("The test passed according to the worker");
    assert!(!entry.accepted());
    entry
        .data
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("object"))?
        .remove("ruling");
    assert!(entry.accepted());
    Ok(())
}

#[test]
fn each_affected_task_needs_its_own_receipt_without_waiting_on_other_tasks() -> anyhow::Result<()> {
    let mut entry = entry()?;
    entry.outcome = "accepted".into();
    entry.ruling_proof = vec![serde_json::from_value(json!({
        "note": entry.note_id, "obligation_id": entry.obligation_id, "task": "T1", "required": true,
        "version": 1, "command": "true", "expected_exit_code": 0,
        "commit": "one", "state": "content", "ledger": "ruling", "check_id": 4,
        "outcome": "passed"
    }))?];
    assert!(entry.accepted_for("T1"));
    assert!(!entry.accepted_for("T2"));
    assert!(!entry.accepted());
    Ok(())
}
