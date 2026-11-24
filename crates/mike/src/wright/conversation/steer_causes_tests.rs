use chrono::Utc;
use serde_json::{Value, json};

use super::steer::{Change, KINDS, parse};

fn parsed(proposal: &Value) -> anyhow::Result<Change> {
    parse(proposal, Utc::now()).map_err(anyhow::Error::msg)
}

fn refusal(proposal: &Value) -> anyhow::Result<String> {
    match parse(proposal, Utc::now()) {
        Err(reason) => Ok(reason),
        Ok(change) => anyhow::bail!("{proposal} should be refused, parsed as {change:?}"),
    }
}

#[test]
fn parse_every_cause_kind_round_trips_through_its_stored_json() -> anyhow::Result<()> {
    let cases = [
        (
            json!({"kind": "pin_cause", "cause_id": 4}),
            Change::PinCause { cause_id: 4 },
        ),
        (
            json!({"kind": "unpin_cause", "cause_id": 4}),
            Change::UnpinCause { cause_id: 4 },
        ),
        (
            json!({"kind": "set_cause_priority", "cause_id": 4, "priority": 12.5}),
            Change::SetCausePriority {
                cause_id: 4,
                priority: Some(12.5),
            },
        ),
        (
            json!({"kind": "set_cause_priority", "cause_id": 4, "priority": null}),
            Change::SetCausePriority {
                cause_id: 4,
                priority: None,
            },
        ),
        (
            json!({"kind": "merge_causes", "into": 298, "cause_ids": [307, 301, 307]}),
            Change::MergeCauses {
                into: 298,
                cause_ids: vec![301, 307],
            },
        ),
        (
            json!({"kind": "separate_merge", "merge_id": 9}),
            Change::SeparateMerge { merge_id: 9 },
        ),
        (
            json!({"kind": "group_causes", "name": " slow memory queries ", "cause_ids": [318, 19]}),
            Change::GroupCauses {
                name: "slow memory queries".to_owned(),
                cause_ids: vec![19, 318],
            },
        ),
        (
            json!({"kind": "ungroup_causes", "cause_ids": [19]}),
            Change::UngroupCauses {
                cause_ids: vec![19],
            },
        ),
    ];
    for (proposal, expected) in cases {
        let change = parsed(&proposal)?;
        assert_eq!(change, expected, "{proposal}");
        assert_eq!(serde_json::from_value::<Change>(change.to_json())?, change);
        let kind = proposal["kind"].as_str().unwrap_or_default();
        assert!(KINDS.contains(&kind), "{kind}");
    }
    Ok(())
}

#[test]
fn parse_cause_kinds_refuse_a_bad_cause_id_with_the_value_named() -> anyhow::Result<()> {
    for kind in ["pin_cause", "unpin_cause"] {
        let reason = refusal(&json!({"kind": kind, "cause_id": 0}))?;
        assert!(reason.contains("steer.cause_id is 0"), "{reason}");
        let reason = refusal(&json!({"kind": kind, "cause_id": "the engineer_limits one"}))?;
        assert!(reason.contains("engineer_limits"), "{reason}");
        let reason = refusal(&json!({"kind": kind}))?;
        assert!(reason.contains("missing"), "{reason}");
    }
    let reason = refusal(&json!({"kind": "separate_merge", "merge_id": -2}))?;
    assert!(reason.contains("steer.merge_id is -2"), "{reason}");
    Ok(())
}

#[test]
fn parse_set_cause_priority_refuses_a_missing_or_out_of_range_priority() -> anyhow::Result<()> {
    let reason = refusal(&json!({"kind": "set_cause_priority", "cause_id": 4}))?;
    assert!(reason.contains("steer.priority is missing"), "{reason}");
    let reason =
        refusal(&json!({"kind": "set_cause_priority", "cause_id": 4, "priority": "high"}))?;
    assert!(reason.contains("\"high\""), "{reason}");
    let reason = refusal(&json!({"kind": "set_cause_priority", "cause_id": 4, "priority": 1e13}))?;
    assert!(reason.contains("between"), "{reason}");
    Ok(())
}

#[test]
fn parse_cause_id_lists_refuse_with_the_position_of_the_bad_entry() -> anyhow::Result<()> {
    let reason = refusal(&json!({"kind": "ungroup_causes", "cause_ids": [3, "x", 5]}))?;
    assert!(reason.contains("steer.cause_ids[1]"), "{reason}");
    let reason = refusal(&json!({"kind": "ungroup_causes", "cause_ids": []}))?;
    assert!(reason.contains("steer.cause_ids is empty"), "{reason}");
    let reason = refusal(&json!({"kind": "ungroup_causes", "cause_ids": 3}))?;
    assert!(reason.contains("steer.cause_ids is 3"), "{reason}");
    let many: Vec<i64> = (1..=201).collect();
    let reason = refusal(&json!({"kind": "ungroup_causes", "cause_ids": many}))?;
    assert!(reason.contains("201"), "{reason}");
    Ok(())
}

#[test]
fn parse_merge_causes_refuses_merging_a_cause_into_itself_and_a_missing_target()
-> anyhow::Result<()> {
    let reason = refusal(&json!({"kind": "merge_causes", "into": 5, "cause_ids": [4, 5]}))?;
    assert!(reason.contains("contains 5"), "{reason}");
    let reason = refusal(&json!({"kind": "merge_causes", "cause_ids": [4]}))?;
    assert!(reason.contains("steer.into is missing"), "{reason}");
    Ok(())
}

#[test]
fn parse_group_causes_refuses_an_empty_or_overlong_name_and_extra_fields() -> anyhow::Result<()> {
    let reason = refusal(&json!({"kind": "group_causes", "name": "  ", "cause_ids": [1]}))?;
    assert!(reason.contains("steer.name"), "{reason}");
    let long = "n".repeat(81);
    let reason = refusal(&json!({"kind": "group_causes", "name": long, "cause_ids": [1]}))?;
    assert!(reason.contains("1 to 80 characters"), "{reason}");
    let reason = refusal(&json!({"kind": "pin_cause", "cause_id": 1, "group_id": 2}))?;
    assert!(reason.contains("steer.group_id"), "{reason}");
    Ok(())
}
