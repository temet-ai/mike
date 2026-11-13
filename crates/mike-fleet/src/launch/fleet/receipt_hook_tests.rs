use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

use super::{Receipt, append_hook, command_of, context_for, slug_of_run, worth_telling};

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_default()
}

fn receipt(seat: &str, started: i64, finished: Option<i64>) -> Receipt {
    Receipt {
        seat: Some(seat.into()),
        command: Some("node --test".into()),
        running: finished.is_none(),
        completion_unknown: false,
        still_valid: None,
        use_id: Some(41),
        changed_since: Vec::new(),
        started_at: Some(at(started)),
        finished_at: finished.map(at),
        is_error: Some(false),
    }
}

#[test]
fn the_slug_is_cut_out_of_a_run_id_and_other_shapes_are_refused() {
    assert_eq!(slug_of_run("fleet-wave-b-mac-9fdad2fd"), Some("wave-b-mac"));
    assert_eq!(
        slug_of_run("fleet-fleet-ref-node-20261027065237-26875480"),
        Some("fleet-ref-node-20261027065237")
    );
    for bad in [
        "wave-b-mac",
        "fleet-",
        "fleet--9fdad2fd",
        "fleet-x-notahexx",
        "fleet-9fdad2fd",
    ] {
        assert_eq!(slug_of_run(bad), None, "{bad}");
    }
}

#[test]
fn only_a_bash_pre_tool_use_event_with_a_command_is_looked_up() {
    let bash = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash",
        "tool_input": {"command": "  node --test  "}});
    assert_eq!(command_of(&bash), Some("node --test"));
    for other in [
        json!({"hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_input": {"command": "x"}}),
        json!({"hook_event_name": "PreToolUse", "tool_name": "Edit", "tool_input": {"command": "x"}}),
        json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "   "}}),
        json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"}),
    ] {
        assert_eq!(command_of(&other), None, "{other}");
    }
}

#[test]
fn a_finished_slow_result_at_an_unchanged_tree_is_told_and_the_rest_are_not() {
    let told = receipt("ts-T1", 100, Some(130));
    assert!(worth_telling(&told));
    let quick = receipt("ts-T1", 100, Some(103));
    assert!(!worth_telling(&quick));
    let running = receipt("ts-T1", 100, None);
    assert!(!worth_telling(&running));
    let mut invalid = receipt("ts-T1", 100, Some(130));
    invalid.still_valid = Some(false);
    assert!(!worth_telling(&invalid));
    let mut changed = receipt("ts-T1", 100, Some(130));
    changed.changed_since = vec!["src/a.ts".into()];
    assert!(!worth_telling(&changed));
    let mut unknown = receipt("ts-T1", 100, Some(130));
    unknown.completion_unknown = true;
    assert!(!worth_telling(&unknown));
}

#[test]
fn the_context_names_the_newest_receipt_its_seat_age_and_where_the_output_is() {
    let now = at(1000);
    let older = receipt("ts-T1", 100, Some(140));
    let mut newest = receipt("ts-T3", 700, Some(760));
    newest.still_valid = Some(true);
    let text = context_for("node --test", &[older, newest], now).unwrap_or_default();
    assert!(text.starts_with("[mike receipt] `node --test` finished 4 min ago in seat ts-T3 after 60s; no path it covers has changed since."), "{text}");
    assert!(text.contains("list_fleet_results (use_id 41)"), "{text}");
    assert!(
        text.ends_with("Run it again only when you need a result newer than that one."),
        "{text}"
    );
}

#[test]
fn a_receipt_for_another_command_is_ignored_and_an_old_one_reads_in_hours() {
    let now = at(100_000);
    let theirs = receipt("ts-T1", 100, Some(130));
    assert_eq!(context_for("ls", std::slice::from_ref(&theirs), now), None);
    let text = context_for("node --test", &[theirs], now).unwrap_or_default();
    assert!(text.contains("finished 27 h ago"), "{text}");
}

#[test]
fn an_unproven_receipt_says_so_and_a_failed_one_says_failed() {
    let now = at(1000);
    let mut failed = receipt("ts-T2", 100, Some(130));
    failed.command = Some("npm test".into());
    failed.is_error = Some(true);
    failed.use_id = None;
    let text = context_for("npm test", &[failed], now).unwrap_or_default();
    assert!(text.contains("`npm test` failed 14 min ago in seat ts-T2 after 30s; no checkout change was recorded since it ran, which does not prove its result still holds."), "{text}");
    assert!(!text.contains("use_id"), "{text}");
    let mut quick = receipt("ts-T2", 100, Some(102));
    quick.command = Some("npm test".into());
    assert_eq!(context_for("npm test", &[quick], now), None);
    assert_eq!(
        context_for("npm test", &[], now + Duration::seconds(1)),
        None
    );
}

#[test]
fn the_hook_is_installed_once_beside_operator_hooks_and_not_when_switched_off() {
    let operator = json!({"matcher": "Bash", "hooks": [{"type": "command", "command": "guard"}]});
    let mut settings = json!({"hooks": {"PreToolUse": [operator.clone()]}});
    let exe = Path::new("/state/it's mike");
    append_hook(&mut settings, exe);
    append_hook(&mut settings, exe);
    assert_eq!(settings["hooks"]["PreToolUse"][0], operator);
    assert_eq!(
        settings["hooks"]["PreToolUse"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        settings["hooks"]["PreToolUse"][1]["hooks"][0]["command"],
        "'/state/it'\\''s mike' fleet receipt-hook"
    );
    assert_eq!(settings["hooks"]["PreToolUse"][1]["hooks"][0]["timeout"], 3);
    for mut off in [
        json!({"env": {super::ENABLED_ENV: "0"}}),
        json!({"disableAllHooks": true}),
    ] {
        let before: Value = off.clone();
        append_hook(&mut off, exe);
        assert_eq!(off, before);
    }
}
