use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

use super::assemble::{StageInput, assemble};
use super::stages::STAGES;

fn input(index: usize, enabled: bool, produced: Option<i64>) -> anyhow::Result<StageInput> {
    let def = STAGES
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("no stage {index}"))?;
    Ok(StageInput {
        def,
        toggle_key: Some("a_toggle"),
        enabled,
        last_ran_at: None,
        running_since: None,
        last_outcome: Some(json!({"outcome": "skipped", "reason": "outside_window"})),
        produced_24h: produced,
        failing: false,
    })
}

fn all_producing() -> anyhow::Result<Vec<StageInput>> {
    (0..STAGES.len())
        .map(|index| input(index, true, STAGES[index].rows.map(|_| 3)))
        .collect()
}

fn since() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 12, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[test]
fn learning_loop_lists_the_twelve_stages_in_order_with_every_field() -> anyhow::Result<()> {
    let body = serde_json::to_value(assemble(since(), all_producing()?))?;
    let ids: Vec<&str> = body["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|stage| stage["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "capture",
            "turn_judge",
            "session_judge",
            "lesson_proposer",
            "lesson_relations",
            "consolidation",
            "learning_promote",
            "lessons_commit",
            "supply",
            "supply_verdict",
            "search",
            "memory_review"
        ]
    );
    assert_eq!(body["broken_at"], Value::Null);
    assert_eq!(body["stages"][7]["label"], "Lesson file commit");
    let proposer = &body["stages"][3];
    assert_eq!(proposer["running_since"], Value::Null);
    for field in [
        "label",
        "kind",
        "toggle_key",
        "enabled",
        "last_ran_at",
        "running_since",
        "last_outcome",
        "produced_24h",
        "fed_by",
        "healthy",
        "run_now",
    ] {
        assert!(proposer.get(field).is_some(), "stage lacks {field}");
    }
    assert_eq!(proposer["kind"], "worker");
    assert_eq!(proposer["fed_by"], "session_judge");
    assert_eq!(
        proposer["run_now"],
        json!({"method": "POST", "path": "/api/lessons/proposer-pass"})
    );
    assert_eq!(body["stages"][4]["kind"], "job");
    Ok(())
}

#[test]
fn learning_loop_breaks_at_the_first_on_stage_that_produced_nothing_after_its_feeder_did()
-> anyhow::Result<()> {
    let mut inputs = all_producing()?;
    inputs[10].produced_24h = Some(0);
    inputs[11].produced_24h = Some(0);
    let body = assemble(since(), inputs);
    assert_eq!(body.broken_at, Some("search"));
    assert!(!body.stages[10].healthy);
    assert!(
        body.stages[11].healthy,
        "a stage whose feeder produced nothing is not blamed"
    );
    Ok(())
}

#[test]
fn learning_loop_does_not_blame_a_stage_that_is_off() -> anyhow::Result<()> {
    let mut inputs = all_producing()?;
    inputs[9] = input(9, false, Some(0))?;
    let body = assemble(since(), inputs);
    assert_eq!(body.broken_at, None);
    Ok(())
}

#[test]
fn learning_loop_breaks_at_a_failing_job_without_rows() -> anyhow::Result<()> {
    let mut inputs = all_producing()?;
    inputs[6].failing = true;
    let body = assemble(since(), inputs);
    assert_eq!(body.broken_at, Some("learning_promote"));
    Ok(())
}
