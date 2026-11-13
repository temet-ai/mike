use serde_json::{Value, json};
use uuid::Uuid;

use super::{at, decision, definition, final_pass, obligation, status};

#[test]
fn malformed_register_records_remain_visible_and_block_signoff() -> anyhow::Result<()> {
    let def = definition()?;
    for field in ["obligation_id", "task", "outcome", "observation"] {
        let mut check = obligation(1, "accepted");
        check.data[field] = Value::Null;
        let found = status(&def, &[check, final_pass(2)]);
        assert!(!found.accepted, "{field}");
        assert_eq!(found.obligations.len(), 1);
        assert!(found.obligations[0].issue.is_some(), "{field}");
    }
    let mut global = obligation(1, "pending");
    global.data = json!({"obligation_id": "global", "outcome": "pending"});
    let found = status(&def, &[global, final_pass(2)]);
    assert!(found.criteria.iter().all(|criterion| criterion.passed == 1));
    assert!(!found.accepted);
    Ok(())
}

#[test]
fn observation_reference_is_required_but_not_claimed_as_proof_of_truth() -> anyhow::Result<()> {
    let def = definition()?;
    let mut check = obligation(1, "accepted");
    check.data["observation"] = Value::Null;
    check.data["observation_notes"] = json!([Uuid::from_u128(900)]);
    assert!(status(&def, &[check.clone(), final_pass(2)]).accepted);
    check.data["observation_notes"] = json!(["not-a-note-id"]);
    assert!(!status(&def, &[check, final_pass(2)]).accepted);
    Ok(())
}

#[test]
fn explicit_legacy_scoped_verdict_remains_usable_without_register() -> anyhow::Result<()> {
    let def = definition()?;
    let found = status(
        &def,
        &[decision(
            1,
            json!({"task": "T1", "criteria": ["C1"], "verdict": "pass"}),
        )],
    );
    assert_eq!(found.criteria[0].passed, 1);
    assert!(found.tasks[0].ready);
    Ok(())
}

#[test]
fn malformed_successors_cannot_launder_original_scope_and_can_be_repaired() -> anyhow::Result<()> {
    let def = definition()?;
    let first = obligation(1, "pending");
    let mut wrong = obligation(2, "pending");
    wrong.data["task"] = json!("T2");
    wrong.data["criteria"] = json!(["C2"]);
    wrong.data["supersedes"] = json!(first.note_uuid);
    let mut laundering = wrong.clone();
    laundering.note_uuid = Uuid::from_u128(3);
    laundering.created_at = at(3);
    laundering.updated_at = at(3);
    laundering.data["supersedes"] = json!(wrong.note_uuid);
    laundering.data["outcome"] = json!("accepted");
    let mut notes = vec![first, wrong, laundering.clone(), final_pass(4)];
    let found = status(&def, &notes);
    assert!(!found.accepted);
    assert!(
        found.obligations[0]
            .issue
            .as_deref()
            .is_some_and(|issue| issue.contains("preserve"))
    );
    let mut repaired = obligation(5, "accepted");
    repaired.data["supersedes"] = json!(laundering.note_uuid);
    notes.extend([repaired, final_pass(6)]);
    assert!(status(&def, &notes).accepted);
    Ok(())
}

#[test]
fn stale_successors_cannot_erase_later_invalidation_in_the_lineage() -> anyhow::Result<()> {
    let def = definition()?;
    let mut first = obligation(1, "invalidated");
    first.updated_at = at(5);
    let mut second = obligation(2, "accepted");
    second.updated_at = at(3);
    second.data["supersedes"] = json!(first.note_uuid);
    let mut third = obligation(3, "accepted");
    third.updated_at = at(4);
    third.data["supersedes"] = json!(second.note_uuid);
    let mut notes = vec![first, second, third.clone(), final_pass(6)];
    assert!(!status(&def, &notes).accepted);
    let mut repaired = obligation(7, "accepted");
    repaired.data["supersedes"] = json!(third.note_uuid);
    notes.extend([repaired, final_pass(8)]);
    assert!(status(&def, &notes).accepted);
    Ok(())
}

#[test]
fn superseded_obligation_blocker_is_not_duplicated_in_legacy_blocked_list() -> anyhow::Result<()> {
    let def = definition()?;
    let mut first = obligation(1, "blocked");
    first.status = Some("blocked".into());
    let mut successor = obligation(2, "accepted");
    successor.data["supersedes"] = json!(first.note_uuid);
    let mut operational = decision(3, json!({}));
    operational.kind = "task".into();
    operational.status = Some("blocked".into());
    operational.title = "worker waiting for a dependency".into();
    let found = status(&def, &[first, successor, operational, final_pass(4)]);
    assert!(found.accepted);
    assert_eq!(found.blocked.len(), 1);
    assert_eq!(found.blocked[0].title, "worker waiting for a dependency");
    Ok(())
}

#[test]
fn malformed_scope_never_reopens_other_tasks_but_blocks_fleet_signoff() -> anyhow::Result<()> {
    let def = definition()?;
    for outcome in ["failed", "pending", "accepted"] {
        for scope in [
            json!({"task": "T1", "criteria": ["C2"]}),
            json!({"task": "unknown", "criteria": ["C1"]}),
            json!({"task": null, "criteria": ["C1"]}),
            json!({}),
        ] {
            let mut check = obligation(3, outcome);
            check.data = scope;
            check.data["obligation_id"] = json!("broken-scope");
            check.data["outcome"] = json!(outcome);
            let found = status(&def, &[final_pass(2), check]);
            assert!(!found.accepted);
            assert!(found.obligations[0].issue.is_some());
            assert!(found.tasks.iter().all(|task| task.state == "passed"));
            assert!(found.tasks.iter().all(|task| task.ready));
            assert!(found.obligations[0].tasks.is_empty());
            assert!(found.obligations[0].criteria.is_empty());
        }
    }
    Ok(())
}

#[test]
fn malformed_successor_keeps_original_scope_without_poisoning_other_tasks() -> anyhow::Result<()> {
    let def = definition()?;
    let first = obligation(1, "pending");
    let mut wrong = obligation(3, "failed");
    wrong.data["task"] = json!("T2");
    wrong.data["criteria"] = json!(["C2"]);
    wrong.data["supersedes"] = json!(first.note_uuid);
    let found = status(&def, &[first, final_pass(2), wrong.clone()]);
    assert!(!found.accepted);
    assert_eq!(found.tasks[0].state, "rework");
    assert_eq!(found.tasks[1].state, "passed");
    assert_eq!(found.obligations[0].tasks, ["T1"]);
    assert_eq!(found.obligations[0].criteria, ["C1"]);
    assert_eq!(found.obligations[0].data, wrong.data);
    Ok(())
}

#[test]
fn valid_scopes_still_reopen_only_their_matching_tasks() -> anyhow::Result<()> {
    let def = definition()?;
    for scope in [
        json!({"task": "T1"}),
        json!({"criteria": ["C1"]}),
        json!({"task": "T1", "criteria": ["C1"]}),
    ] {
        let mut check = obligation(3, "failed");
        check.data = scope;
        check.data["obligation_id"] = json!("valid-scope");
        check.data["outcome"] = json!("failed");
        let found = status(&def, &[final_pass(2), check]);
        assert_eq!(found.tasks[0].state, "rework");
        assert_eq!(found.tasks[1].state, "passed");
        assert!(!found.accepted);
    }
    Ok(())
}

#[test]
fn later_slice_pass_clears_rework_without_granting_final_acceptance() -> anyhow::Result<()> {
    let def = definition()?;
    let fail = decision(
        2,
        json!({"task": "T1", "criteria": ["C1"], "verdict": "fail", "scope": "slice"}),
    );
    for scope in [json!({"scope": "slice"}), json!({"slice": "setting save"})] {
        let mut pass = decision(3, scope);
        pass.data["task"] = json!("T1");
        pass.data["criteria"] = json!(["C1"]);
        pass.data["verdict"] = json!("pass");
        let found = status(&def, &[obligation(1, "pending"), fail.clone(), pass]);
        assert_ne!(found.tasks[0].state, "rework");
        assert!(!found.accepted);
        assert!(found.tasks[0].passed_criteria.is_empty());
        assert_eq!(found.tasks[0].fails, 1);
        assert_eq!(found.tasks[0].passes, 1);
    }
    Ok(())
}

#[test]
fn slice_pass_does_not_clear_another_criterion_or_failed_obligation() -> anyhow::Result<()> {
    let mut def = definition()?;
    def.tasks
        .get_mut("T1")
        .ok_or_else(|| anyhow::anyhow!("T1"))?
        .criteria
        .push("C2".into());
    let fail = decision(
        2,
        json!({"task": "T1", "criteria": ["C1"], "verdict": "fail", "scope": "slice"}),
    );
    let pass = decision(
        3,
        json!({"task": "T1", "criteria": ["C2"], "verdict": "pass", "scope": "slice"}),
    );
    assert_eq!(status(&def, &[fail, pass.clone()]).tasks[0].state, "rework");
    let mut same = pass;
    same.data["criteria"] = json!(["C1"]);
    assert_eq!(
        status(&def, &[obligation(1, "failed"), same]).tasks[0].state,
        "rework"
    );
    Ok(())
}

#[test]
fn invalid_register_does_not_change_legacy_verdict_protocol_for_unrelated_tasks()
-> anyhow::Result<()> {
    let def = definition()?;
    let mut invalid = obligation(2, "failed");
    invalid.data["task"] = json!("unknown");
    let legacy = decision(
        1,
        json!({"tasks": ["T1", "T2"], "criteria": ["C1", "C2"], "verdict": "pass"}),
    );
    let found = status(&def, &[legacy, invalid]);
    assert!(found.tasks.iter().all(|task| task.state == "passed"));
    assert!(!found.accepted);
    Ok(())
}
