use super::*;
use chrono::TimeZone;
use serde_json::json;

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap_or_default()
}

fn row(command: &str) -> TreeResultRow {
    TreeResultRow {
        use_id: 17,
        seat: Some("predecessor-T1".into()),
        session_uuid: Some(Uuid::from_u128(1)),
        command: Some(command.into()),
        started_at: Some(at(100)),
        finished_at: Some(at(200)),
        is_error: Some(false),
        content: Some(json!([{"type": "text", "text": "all captured output"}])),
    }
}

#[test]
fn a_captured_command_is_never_read_as_test_proof() {
    for command in ["cargo check --all-targets", "npm test", "pytest"] {
        let proof = from_rows(&[row(command)], Some(&[]));
        assert!(proof.receipts[0].kinds.is_empty());
        assert!(proof.body().contains("No currently valid tests run"));
    }
}

#[test]
fn receipts_preserve_command_owner_and_full_output_access_without_claiming_commit_binding() {
    let result = row("cargo nextest run -p lib; cargo clippy -p lib");
    let proof = from_rows(std::slice::from_ref(&result), Some(&[]));
    assert_eq!(
        proof.receipts[0].command,
        result.command.unwrap_or_default()
    );
    assert_eq!(
        proof.receipts[0].full_output["tool"],
        "list_session_tool_uses"
    );
    assert_eq!(proof.receipts[0].full_output["find_use_id"], 17);
    assert_eq!(
        proof.receipts[0].full_output["arguments"]["include_results"],
        true
    );
    assert_eq!(proof.receipts[0].seat.as_deref(), Some("predecessor-T1"));
    assert!(proof.body().contains("Commit binding: unknown"));
    assert!(
        proof
            .body()
            .contains("Tool success alone does not prove tests passed")
    );
}

#[test]
fn changes_during_or_after_a_run_invalidate_receipts() {
    for seen_at in [at(100), at(150), at(250)] {
        let changes = [ChangedPathRow {
            path: "src/lib.rs".into(),
            seen_at,
        }];
        let proof = from_rows(&[row("cargo test")], Some(&changes));
        assert_eq!(proof.receipts[0].still_valid, Some(false));
        assert_eq!(proof.receipts[0].paths_changed_since_started, 1);
        assert_eq!(
            proof.receipts[0].changed_paths["tool"],
            "list_fleet_changes"
        );
    }
}

#[test]
fn a_busy_shared_tree_does_not_copy_its_changed_paths_into_every_receipt() {
    let changes: Vec<ChangedPathRow> = (0..2_000)
        .map(|n| ChangedPathRow {
            path: format!("crates/radar-core/src/tmp{n:05}"),
            seen_at: at(150),
        })
        .collect();
    let rows: Vec<_> = (0..74).map(|_| row("cargo test")).collect();
    let proof = from_rows(&rows, Some(&changes));
    assert_eq!(proof.receipts[0].paths_changed_since_started, 2_000);
    let size = serde_json::to_string(&proof).map_or(usize::MAX, |text| text.len());
    assert!(size < 200_000, "{size} bytes for 74 receipts");
}

#[test]
fn unknown_history_running_and_failed_commands_never_hide_their_status() {
    let mut result = row("cargo test");
    result.is_error = Some(true);
    let proof = from_rows(&[result.clone()], None);
    assert_eq!(proof.receipts[0].still_valid, None);
    assert_eq!(proof.receipts[0].tool_verdict, "failed");
    result.finished_at = None;
    let proof = from_rows(&[result], Some(&[]));
    assert_eq!(proof.receipts[0].still_valid, None);
    assert!(
        proof.receipts[0]
            .tool_verdict
            .contains("completion unknown")
    );
}

#[test]
fn update_mode_runs_are_not_reusable_verification() {
    for command in [
        "GG_TS_UPDATE=1 cargo nextest run",
        "cargo test -- --update",
        "INSTA_UPDATE=always cargo test",
    ] {
        let proof = from_rows(&[row(command)], Some(&[]));
        assert!(proof.receipts[0].update_mode);
        assert_eq!(proof.receipts[0].still_valid, Some(false));
    }
}

#[test]
fn wrapped_commands_retain_full_command() {
    let proof = from_rows(&[row("scripts/cargo-slot cargo clippy -p lib")], Some(&[]));
    assert_eq!(
        proof.receipts[0].command,
        "scripts/cargo-slot cargo clippy -p lib"
    );
}

#[test]
fn query_bounds_and_unavailable_proof_are_visible() {
    let proof = ReportProof {
        since: Some(at(0)),
        unavailable: Some("database unavailable".into()),
        ..ReportProof::default()
    };
    let body = proof.body();
    assert!(body.contains("database unavailable"));
    assert!(body.contains("row_limit"));
    assert!(body.contains("No currently valid tests run"));
}

#[test]
fn rework_receipts_include_predecessors_but_not_other_tasks() {
    let first = row("cargo nextest run -p lib");
    let mut other = row("cargo clippy -p other");
    other.seat = Some("worker-T10".into());
    let mut proof = from_rows(&[first, other], Some(&[]));
    proof.retain_task("T1");
    assert_eq!(proof.receipts.len(), 1);
    assert_eq!(proof.receipts[0].seat.as_deref(), Some("predecessor-T1"));
}

#[test]
fn missing_session_retains_full_output_instead_of_unusable_link() {
    let mut result = row("cargo test");
    result.session_uuid = None;
    let proof = from_rows(std::slice::from_ref(&result), Some(&[]));
    assert_eq!(
        proof.receipts[0].full_output["captured_output"],
        result.content.unwrap_or_default()
    );
}

#[test]
fn successor_handoff_bounds_receipts_and_never_embeds_captured_output() {
    let mut result = row(&format!("cargo test {}", "界".repeat(1_000)));
    result.session_uuid = None;
    result.content = Some(json!({"text": "large captured output".repeat(1_000)}));
    let mut proof = from_rows(&vec![result; 200], Some(&[]));
    proof.unavailable = Some("unavailable details".repeat(1_000));
    let handoff = proof.handoff_body();
    assert!(handoff.len() < 12_000, "{} bytes", handoff.len());
    assert!(handoff.contains("200 available, 6 shown"));
    assert!(handoff.contains("command_truncated\":false"));
    assert!(handoff.contains("command_excerpt\":null"));
    assert!(handoff.contains("full_command_and_output\":null"));
    assert!(handoff.contains("list_session_tool_uses"));
    assert!(handoff.contains("Commit binding is unknown"));
    assert!(!handoff.contains("large captured output"));
    assert_eq!(proof.receipts.len(), 200);
    assert!(proof.receipts[0].full_output["captured_output"].is_object());
}

#[test]
fn empty_or_missing_change_history_never_proves_a_captured_result_reusable() {
    let empty: &[ChangedPathRow] = &[];
    for history in [None, Some(empty)] {
        let proof = from_rows(&[row("cargo test")], history);
        let receipt = &proof.receipts[0];
        assert_eq!(receipt.still_valid, None);
        assert_eq!(
            receipt.tool_verdict,
            "tool succeeded; inspect output for test or lint verdict"
        );
        assert_eq!(receipt.full_output["find_use_id"], 17);
        assert!(proof.body().contains("no observed changes yields null"));
    }
}

#[test]
fn update_mode_invalidates_even_when_change_capture_is_unavailable() {
    let proof = from_rows(&[row("GG_TS_UPDATE=1 cargo nextest run")], None);
    assert_eq!(proof.receipts[0].still_valid, Some(false));
    assert!(proof.receipts[0].update_mode);
}
