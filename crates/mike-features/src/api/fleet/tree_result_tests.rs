use super::{RESULT_VALIDITY_PROVENANCE, ResultsView, result_view};
use crate::store::persistence::fleet::TreeResultRow;
use chrono::{Duration, Utc};
use serde_json::json;

fn forty_commands() -> Vec<TreeResultRow> {
    let finished = Utc::now() - Duration::minutes(5);
    (0..40)
        .map(|n| TreeResultRow {
            use_id: n,
            seat: Some(format!("rust-developer-T{}", n % 6 + 1)),
            session_uuid: Some(uuid::Uuid::from_u128(n as u128 + 1)),
            command: Some(format!(
                "scripts/cx nextest run -p mike --lib launch::fleet::case_{n}"
            )),
            started_at: Some(finished - Duration::seconds(30)),
            finished_at: Some(finished),
            is_error: Some(false),
            content: Some(json!("line of output\n".repeat(30))),
        })
        .collect()
}

#[test]
fn the_default_result_of_forty_commands_has_no_output_and_is_under_1500_tokens()
-> anyhow::Result<()> {
    let rows: Vec<_> = forty_commands()
        .into_iter()
        .map(|row| result_view(row, &[], &[], false))
        .collect();
    let text = serde_json::to_string(&rows)?;
    assert!(!text.contains("output_tail"), "{text}");
    assert!(!text.contains("line of output"), "{text}");
    let tokens = text.chars().count() as f64 / 3.5;
    assert!(tokens < 1500.0, "{tokens} tokens: {text}");
    Ok(())
}

#[test]
fn a_result_carries_its_command_seat_validity_and_running_flag_by_default() -> anyhow::Result<()> {
    let row = result_view(forty_commands().remove(0), &[], &[], false);
    let value = serde_json::to_value(&row)?;
    assert_eq!(value["seat"], "rust-developer-T1");
    assert!(
        value["command"]
            .as_str()
            .is_some_and(|c| c.contains("case_0"))
    );
    assert!(value.get("running").is_none(), "absent means not running");
    assert_eq!(value["still_valid"], serde_json::Value::Null);
    Ok(())
}

#[test]
fn a_command_queued_for_a_build_slot_shows_as_waiting_not_running() -> anyhow::Result<()> {
    let mut open = forty_commands().remove(0);
    open.finished_at = None;
    let queued = result_view(open.clone(), &[], &[], false).waited(Some(95));
    let value = serde_json::to_value(&queued)?;
    assert_eq!(value["waiting"], true);
    assert_eq!(value["waiting_seconds"], 95);
    assert!(value.get("running").is_none(), "{value}");
    let value = serde_json::to_value(result_view(open, &[], &[], false).waited(None))?;
    assert_eq!(value["running"], true);
    assert!(value.get("waiting").is_none(), "{value}");
    Ok(())
}

#[test]
fn asking_for_output_returns_the_tail_and_the_times() -> anyhow::Result<()> {
    let row = result_view(forty_commands().remove(0), &[], &[], true);
    let value = serde_json::to_value(&row)?;
    assert!(
        value["output_tail"]
            .as_str()
            .is_some_and(|t| t.contains("line of output"))
    );
    assert!(value["started_at"].is_string());
    assert!(value["session"].is_string());
    Ok(())
}
#[test]
fn an_unconfirmed_background_completion_is_neither_passed_nor_reported_live() -> anyhow::Result<()>
{
    let mut row = forty_commands().remove(0);
    row.finished_at = None;
    row.is_error = None;
    row.content = Some(serde_json::json!(
        "Command running in background with ID: pending"
    ));
    let value = serde_json::to_value(result_view(row, &[], &[], true).waited(Some(3600)))?;
    assert_eq!(value["completion_unknown"], true);
    assert_eq!(value["still_valid"], serde_json::Value::Null);
    for absent in [
        "running",
        "waiting",
        "waiting_seconds",
        "finished_at",
        "is_error",
    ] {
        assert!(value.get(absent).is_none(), "{absent}: {value}");
    }
    Ok(())
}

#[test]
fn result_provenance_is_explicit_without_repeating_it_in_every_row() -> anyhow::Result<()> {
    let results = forty_commands()
        .into_iter()
        .map(|row| result_view(row, &[], &[], false))
        .collect();
    let view = ResultsView {
        run_id: "fleet-results-run".into(),
        paths: Vec::new(),
        validity_provenance: RESULT_VALIDITY_PROVENANCE,
        results,
    };
    let text = serde_json::to_string(&view)?;
    let value = serde_json::to_value(&view)?;
    assert!(
        value["validity_provenance"]
            .as_str()
            .is_some_and(|p| p.contains("no commit binding"))
    );
    assert!(
        value["validity_provenance"]
            .as_str()
            .is_some_and(|p| p.contains("Tool success is not a test pass"))
    );
    assert_eq!(
        value["results"][0].get("still_valid"),
        Some(&serde_json::Value::Null)
    );
    assert!(text.chars().count() as f64 / 3.5 < 1500.0, "{text}");
    Ok(())
}

#[test]
fn raw_bash_changes_since_start_invalidate_even_outside_the_requested_paths() -> anyhow::Result<()>
{
    let row = forty_commands().remove(0);
    let started = row
        .started_at
        .ok_or_else(|| anyhow::anyhow!("fixture start missing"))?;
    for seconds in [0, 15, 120] {
        let changes = [crate::store::persistence::fleet::ChangedPathRow {
            path: "crates/gg-core/src/host.rs".into(),
            seen_at: started + Duration::seconds(seconds),
        }];
        let filtered = serde_json::to_value(result_view(
            row.clone(),
            &changes,
            &["crates/gg-ui".into()],
            true,
        ))?;
        assert_eq!(filtered["still_valid"], false);
        assert!(filtered.get("changed_since").is_none());
        let all = serde_json::to_value(result_view(row.clone(), &changes, &[], true))?;
        assert_eq!(all["changed_since"], json!(["crates/gg-core/src/host.rs"]));
        assert_eq!(all["use_id"], row.use_id);
    }
    Ok(())
}

#[test]
fn raw_bash_missing_change_events_never_prove_commit_bound_reuse() -> anyhow::Result<()> {
    let mut row = forty_commands().remove(0);
    for error in [false, true] {
        row.is_error = Some(error);
        let value = serde_json::to_value(result_view(row.clone(), &[], &[], true))?;
        assert!(value["still_valid"].is_null());
        assert_eq!(value["is_error"], error);
        assert!(value.get("passed").is_none());
    }
    row.started_at = None;
    let value = serde_json::to_value(result_view(row, &[], &[], true))?;
    assert!(value["still_valid"].is_null());
    Ok(())
}

#[test]
fn raw_bash_update_mode_is_invalid_without_relying_on_change_events() -> anyhow::Result<()> {
    let mut row = forty_commands().remove(0);
    row.command = Some("GG_TS_UPDATE=1 cargo nextest run".into());
    let value = serde_json::to_value(result_view(row, &[], &[], false))?;
    assert_eq!(value["still_valid"], false);
    Ok(())
}
