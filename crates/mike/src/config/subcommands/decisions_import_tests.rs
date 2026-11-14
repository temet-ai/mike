use serde_json::{Value, json};

use super::{ReplayKind, import_rows, replay_row};

fn line() -> Value {
    json!({
        "schema": "decisions_replay_v1",
        "point": "wake_recheck",
        "question_set": "wake_recheck_v1",
        "item_id": "u1",
        "source_ref": "decisions_replay:wake_recheck:wake_recheck_v1:u1",
        "item_at": "2026-11-01T09:00:00Z",
        "asked_at": "2026-11-14T12:00:00Z",
        "label": "wrong",
        "request": {
            "model": "typesafe/jev-1.14",
            "state": {"nudge": "wait for the build"},
            "questions": {"concern_still_open": {"type": "noul"}},
        },
        "answers": {"concern_still_open": {"type": "noul", "noul": 0.1}},
        "error": null,
        "model": "typesafe/jev-1.14",
        "latency_ms": 420,
        "cost_usd": 0.002,
        "composed": {"verdict": {"score": 0.9, "skip": true}, "answers": {}, "error": null},
        "score": 0.9,
        "outcome": {"label": "right", "source": "turn_guard_label"},
        "agrees_current": null,
    })
}

#[test]
fn replay_row_maps_the_line_to_a_replay_record_with_composed_and_outcome() -> anyhow::Result<()> {
    let row = replay_row(1, &line())?;
    assert_eq!(row.point, "wake_recheck");
    assert_eq!(row.question_set, "wake_recheck_v1");
    assert_eq!(
        row.source_ref,
        "decisions_replay:wake_recheck:wake_recheck_v1:u1"
    );
    assert_eq!(row.state, json!({"nudge": "wait for the build"}));
    assert_eq!(row.model, "typesafe/jev-1.14");
    assert_eq!(row.latency_ms, Some(420));
    assert_eq!(row.cost_usd, Some(0.002));
    assert_eq!(row.label, Some(("right".to_owned(), "turn_guard_label")));
    assert_eq!(
        row.composed.as_ref().map(|c| c["verdict"]["skip"].clone()),
        Some(json!(true))
    );
    assert!(row.answers.is_some());
    Ok(())
}

#[test]
fn replay_row_of_a_failed_line_has_no_answers_composed_or_label() -> anyhow::Result<()> {
    let mut failed = line();
    failed["answers"] = Value::Null;
    failed["composed"] = Value::Null;
    failed["outcome"] = Value::Null;
    failed["error"] = json!("timeout");
    let row = replay_row(1, &failed)?;
    assert!(row.answers.is_none());
    assert!(row.composed.is_none());
    assert!(row.label.is_none());
    Ok(())
}

#[test]
fn replay_row_rejects_an_unknown_point_with_the_line_number() {
    let mut other = line();
    other["point"] = json!("steerer_checklist");
    let error = replay_row(7, &other).err().map(|e| e.to_string());
    assert_eq!(
        error.as_deref(),
        Some("line 7: `steerer_checklist` is not a replay point")
    );
}

#[test]
fn replay_row_rejects_a_line_of_another_schema() {
    let mut other = line();
    other["schema"] = json!("jev_precheck_replay");
    assert!(replay_row(2, &other).is_err());
}

#[test]
fn import_rows_of_replay_kind_stops_at_the_first_bad_line() {
    let mut bad = line();
    bad["point"] = json!("nope");
    let result = import_rows(ReplayKind::Replay, &[line(), bad]);
    assert!(result.is_err());
}
