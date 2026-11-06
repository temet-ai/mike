use super::*;
use serde_json::json;

fn proof() -> anyhow::Result<RulingProof> {
    Ok(serde_json::from_value(json!({
        "note": Uuid::nil(), "obligation_id": "Q29", "task": "T1", "required": true,
        "version": 1, "command": "test -f ids.txt", "expected_exit_code": 0,
        "commit": "abc", "state": "state", "ledger": "run#ruling:one#baseline:abc:command",
        "outcome": "missing"
    }))?)
}

fn row() -> TreeCheckRow {
    TreeCheckRow {
        id: 12,
        run_id: "run#ruling:one#baseline:abc:command".into(),
        state: "state".into(),
        state_after: Some("state".into()),
        command: "contract".into(),
        started_at: DateTime::default(),
        finished_at: Some(DateTime::default()),
        exit_code: Some(0),
        output: "all output".into(),
        askers: Vec::new(),
        told: Vec::new(),
    }
}

#[test]
fn ruling_proof_accepts_only_exact_clean_finished_receipt() -> anyhow::Result<()> {
    let mut proof = proof()?;
    observed(&mut proof, &row(), "contract");
    assert!(proof.passed());
    assert_eq!(proof.output.as_deref(), Some("all output"));
    assert_eq!(proof.check_id, Some(12));
    Ok(())
}

#[test]
fn zero_exit_without_provenance_is_never_ruling_proof() -> anyhow::Result<()> {
    let original = row();
    let variants = [
        TreeCheckRow {
            state_after: None,
            ..original.clone()
        },
        TreeCheckRow {
            state_after: Some("changed".into()),
            ..original.clone()
        },
        TreeCheckRow {
            state: "other".into(),
            ..original.clone()
        },
        TreeCheckRow {
            run_id: "default check".into(),
            ..original.clone()
        },
        TreeCheckRow {
            command: "another command".into(),
            ..original.clone()
        },
        TreeCheckRow {
            finished_at: None,
            ..original.clone()
        },
        TreeCheckRow {
            exit_code: None,
            ..original
        },
    ];
    for row in variants {
        let mut proof = proof()?;
        observed(&mut proof, &row, "contract");
        assert!(!proof.passed());
        assert_eq!(proof.outcome, "unverifiable");
    }
    Ok(())
}

#[test]
fn executed_contract_failure_is_retained_as_failed_not_missing() -> anyhow::Result<()> {
    let mut proof = proof()?;
    observed(
        &mut proof,
        &TreeCheckRow {
            exit_code: Some(1),
            ..row()
        },
        "contract",
    );
    assert!(!proof.passed());
    assert_eq!(proof.outcome, "failed");
    assert_eq!(proof.check_id, Some(12));
    assert!(proof.problem.is_none());
    Ok(())
}

#[test]
fn a_ruling_whose_nextest_filter_ran_no_test_is_a_definition_problem() -> anyhow::Result<()> {
    let contract =
        "sh -c 'scripts/cx nextest run -p mike --lib store::launch'; mike_ruling_exit=$?";
    let mut proof = proof()?;
    observed(
        &mut proof,
        &TreeCheckRow {
            exit_code: Some(1),
            command: contract.into(),
            output: "error: no tests to run\n[mike ruling] observed exit: 4; expected: 0\n".into(),
            ..row()
        },
        contract,
    );
    assert!(!proof.passed());
    assert_eq!(proof.outcome, "definition_problem");
    assert!(proof.problem.as_deref().is_some_and(
        |why| why.contains("obligation `Q29`") && why.contains("not a failure of the task")
    ));
    Ok(())
}
