use chrono::{DateTime, Duration};
use mike::launch::fleet::introduce::{CommandTiming, mail, repeats, repeats_with_timings};
use mike::launch::fleet::report_proof::{captured_result_validity, from_rows};
use mike::store::persistence::fleet::{ChangedPathRow, SlowCommandRow, TreeResultRow};
use uuid::Uuid;

fn captured(use_id: i64, minutes: i64, command: &str) -> SlowCommandRow {
    let started_at =
        DateTime::from_timestamp(1_700_000_000, 0).unwrap_or_default() + Duration::minutes(minutes);
    SlowCommandRow {
        use_id,
        seat: format!("worker-{use_id}"),
        session: Uuid::from_u128(u128::try_from(use_id).unwrap_or_default()),
        started_at,
        finished_at: started_at + Duration::seconds(185),
        is_error: false,
        in_background: false,
        command: command.into(),
    }
}

fn timing(use_id: i64, work: &str, seconds: f64) -> CommandTiming {
    CommandTiming {
        use_id,
        work: work.into(),
        seconds,
    }
}

#[test]
fn timing_contract_requires_measured_finite_five_seconds_for_both_commands() {
    let work = "cargo clippy -p gg-sessions --all-features";
    let commands = [captured(1, 0, work), captured(2, 5, work)];
    assert!(repeats(&commands, &[]).is_empty());
    for seconds in [0.5, 4.999, f64::NAN, f64::INFINITY] {
        for (first, again) in [(seconds, 185.0), (170.0, seconds)] {
            let measured = [timing(1, work, first), timing(2, work, again)];
            assert!(repeats_with_timings(&commands, &[], &measured).is_empty());
        }
    }
    assert!(repeats_with_timings(&commands, &[], &[timing(2, work, 185.0)]).is_empty());
    let measured = [timing(1, work, 5.0), timing(2, work, 5.0)];
    let found = repeats_with_timings(&commands, &[], &measured);
    assert_eq!(found.len(), 1);
    let body = mail(&found[0], &[]);
    assert!(body.contains("No contact with the first seat is required"));
    assert!(body.contains("\"find_use_id\":1"));
    assert!(body.contains("\"commit\":null"));
    assert!(body.contains("\"still_valid\":null"));
    assert!(!body.contains("and it passed"));
}

#[test]
fn timing_contract_does_not_transfer_compound_or_other_use_duration_to_a_build() {
    let work = "cargo build --bin gg";
    let compound = "cargo build --bin gg; sleep 180";
    let commands = [captured(1, 0, compound), captured(2, 5, work)];
    for first in [
        timing(1, compound, 185.0),
        timing(1, work, 0.5),
        timing(3, work, 185.0),
    ] {
        assert!(repeats_with_timings(&commands, &[], &[first, timing(2, work, 10.0)]).is_empty());
    }
}

#[test]
fn raw_capture_contract_preserves_output_access_without_proving_a_commit_or_test_pass() {
    let command = captured(1, 0, "cargo test");
    let row = TreeResultRow {
        use_id: command.use_id,
        seat: Some(command.seat),
        session_uuid: Some(command.session),
        command: Some(command.command),
        started_at: Some(command.started_at),
        finished_at: Some(command.finished_at),
        is_error: Some(false),
        content: Some(serde_json::json!(
            "test result: FAILED; wrapper exit was zero"
        )),
    };
    assert_eq!(captured_result_validity(&row, None), None);
    assert_eq!(captured_result_validity(&row, Some(&[])), None);
    let proof = from_rows(std::slice::from_ref(&row), Some(&[]), &[]);
    assert_eq!(proof.receipts[0].still_valid, None);
    assert_eq!(proof.receipts[0].full_output["find_use_id"], 1);
    assert!(proof.receipts[0].tool_verdict.contains("inspect output"));
    for seconds in [0, 60, 300] {
        let changes = [ChangedPathRow {
            path: "crates/gg-core/src/host.rs".into(),
            seen_at: command.started_at + Duration::seconds(seconds),
        }];
        assert_eq!(captured_result_validity(&row, Some(&changes)), Some(false));
    }
    let updated = TreeResultRow {
        command: Some("GG_TS_UPDATE=1 cargo nextest run".into()),
        ..row
    };
    assert_eq!(captured_result_validity(&updated, None), Some(false));
}

#[cfg(feature = "test-seams")]
#[test]
fn bounded_handoff_pairs_each_command_excerpt_with_full_retrieval() -> anyhow::Result<()> {
    let command = format!("cargo test {}", "界".repeat(1_000));
    for session in [Some(Uuid::new_v4()), None] {
        let row = TreeResultRow {
            use_id: 42,
            seat: Some("rust-T1".into()),
            session_uuid: session,
            command: Some(command.clone()),
            started_at: None,
            finished_at: None,
            is_error: None,
            content: Some(serde_json::json!({"text": "large captured output".repeat(1_000)})),
        };
        let proof = from_rows(&vec![row; 200], Some(&[]), &[]);
        let body = mike::launch::fleet::test_seams::proof_handoff(&proof);
        assert!(body.len() < 12_000);
        assert!(!body.contains("large captured output"));
        assert!(body.contains("200 available, 6 shown"));
        assert_eq!(proof.receipts.len(), 200);
        assert_eq!(proof.receipts[0].command, command);
        let (_, encoded) = body
            .split_once('\n')
            .ok_or_else(|| anyhow::anyhow!("handoff JSON missing"))?;
        let data: serde_json::Value = serde_json::from_str(encoded)?;
        assert_handoff_reference(&data["receipts"][0], session);
    }
    Ok(())
}

#[cfg(feature = "test-seams")]
fn assert_handoff_reference(row: &serde_json::Value, session: Option<Uuid>) {
    let reference = &row["full_command_and_output"];
    if let Some(session) = session {
        assert_eq!(row["command_truncated"], true);
        assert_eq!(
            row["command_excerpt"].as_str().map(|s| s.chars().count()),
            Some(192)
        );
        assert_eq!(reference["tool"], "list_session_tool_uses");
        assert_eq!(reference["arguments"]["id"], session.to_string());
        assert_eq!(reference["arguments"]["include_results"], true);
        assert_eq!(reference["find_use_id"], 42);
    } else {
        assert_eq!(row["command_truncated"], false);
        assert!(row["command_excerpt"].is_null());
        assert!(reference.is_null());
    }
}
