use super::*;

#[test]
fn final_pass_and_accepted_prose_do_not_bypass_required_executable_ruling() -> anyhow::Result<()> {
    let def = definition()?;
    let mut rule = obligation(1, "accepted");
    rule.data["ruling"] =
        json!({"version": 1, "command": "scripts/check-ids", "expected_exit_code": 0});
    let found = status(&def, &[rule, final_pass(2)]);
    assert!(!found.accepted);
    assert_ne!(found.tasks[0].state, "passed");
    assert!(found.obligations[0].ruling_proof.is_empty());
    Ok(())
}

#[test]
fn successor_cannot_remove_the_executable_contract_or_reuse_its_version() -> anyhow::Result<()> {
    let def = definition()?;
    let mut first = obligation(1, "pending");
    first.data["ruling"] =
        json!({"version": 1, "command": "scripts/check-ids", "expected_exit_code": 0});
    for ruling in [json!(null), first.data["ruling"].clone()] {
        let mut next = obligation(2, "accepted");
        next.data["supersedes"] = json!(first.note_uuid);
        next.data["ruling"] = ruling;
        let found = status(&def, &[first.clone(), next, final_pass(3)]);
        assert!(!found.accepted);
        assert!(
            found.obligations[0]
                .issue
                .as_deref()
                .is_some_and(|issue| issue.contains("higher version"))
        );
    }
    Ok(())
}
