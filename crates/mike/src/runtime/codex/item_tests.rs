use std::collections::HashSet;

use serde_json::json;

use super::test_support::{first_item, item_delta, item_event, observation, scope};
use super::*;

fn apply(
    current: Option<&ProjectedItem>,
    event: &ItemObservation,
) -> anyhow::Result<ProjectedItem> {
    reduce_item(current, event, ReceiptApplication::Unapplied)
        .item
        .ok_or_else(|| anyhow::anyhow!("item fixture did not materialize"))
}

#[test]
fn deltas_identical_adjacent_receipts_both_append_but_old_replay_does_not() -> anyhow::Result<()> {
    let first = item_delta("ha")?;
    let second = item_delta("ha")?;
    let mut ledger = HashSet::new();
    let mut current = None;
    for event in [&first, &second, &first] {
        let replay = if ledger.contains(&event.evidence.receipt) {
            ReceiptApplication::AlreadyApplied
        } else {
            ReceiptApplication::Unapplied
        };
        current = reduce_item(current.as_ref(), event, replay).item;
        ledger.insert(event.evidence.receipt);
    }
    let item = current.ok_or_else(|| anyhow::anyhow!("missing fixture item"))?;
    assert_eq!(
        item.display_text
            .get(&TextChannel::Message)
            .map(String::as_str),
        Some("haha")
    );
    assert_eq!(item.first_evidence, first.evidence);
    assert_eq!(item.latest_evidence, second.evidence);
    assert_eq!(ledger.len(), 2);
    Ok(())
}

#[test]
fn completion_authoritative_text_replaces_deltas_and_late_delta_stays_evidence()
-> anyhow::Result<()> {
    let current = apply(None, &item_delta("draft")?)?;
    let completed = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"agentMessage","text":"final"}),
    )?;
    let current = apply(Some(&current), &completed)?;
    assert_eq!(current.display_text[&TextChannel::Message], "final");
    assert_eq!(current.streamed_text[&TextChannel::Message], "draft");
    assert!(current.terminal);
    let late = item_delta(" late")?;
    let result = reduce_item(Some(&current), &late, ReceiptApplication::Unapplied);
    assert_eq!(result.status, ReductionStatus::PreservedTerminal);
    let current = result.item.ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert_eq!(current.display_text[&TextChannel::Message], "final");
    assert_eq!(current.snapshot_evidence, Some(completed.evidence));
    assert_eq!(current.latest_evidence, late.evidence);
    assert_eq!(current.streamed_text[&TextChannel::Message], "draft late");
    assert!(current.reconciliation_required);
    Ok(())
}

#[test]
fn completed_item_late_start_does_not_regress_snapshot_or_lifecycle() -> anyhow::Result<()> {
    let completed = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"agentMessage","text":"final"}),
    )?;
    let current = apply(None, &completed)?;
    let started = item_event(
        "item/started",
        json!({"id":"Item:1","type":"agentMessage","text":""}),
    )?;
    let result = reduce_item(Some(&current), &started, ReceiptApplication::Unapplied);
    assert_eq!(result.status, ReductionStatus::PreservedTerminal);
    let item = result.item.ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert!(item.terminal);
    assert_eq!(item.display_text[&TextChannel::Message], "final");
    assert_eq!(item.snapshot, current.snapshot);
    Ok(())
}

#[test]
fn partial_completion_preserves_known_fields_without_fabricating_exit_code() -> anyhow::Result<()> {
    let started = item_event(
        "item/started",
        json!({
            "id":"Item:1","type":"commandExecution","command":"sleep 600","cwd":"/work",
            "processId":"process-not-item","status":"inProgress","commandActions":[],"future":{"yes":true}
        }),
    )?;
    let current = apply(None, &started)?;
    assert!(!current.terminal);
    let completed = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"commandExecution","status":"failed","exitCode":null}),
    )?;
    let current = apply(Some(&current), &completed)?;
    let snapshot = current
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("missing snapshot"))?;
    let command = snapshot
        .command
        .ok_or_else(|| anyhow::anyhow!("missing command"))?;
    assert_eq!(command.command, Field::Value("sleep 600".into()));
    assert_eq!(command.process_id, Field::Value("process-not-item".into()));
    assert_eq!(command.exit_code, Field::Null);
    assert_eq!(command.duration_ms, Field::Missing);
    assert_eq!(snapshot.provider_payload["future"]["yes"], true);
    assert_eq!(snapshot.started_at_ms, Field::Value(1720000000123i64));
    assert_eq!(snapshot.completed_at_ms, Field::Value(1720000004567i64));
    Ok(())
}

#[test]
fn history_snapshot_replaces_provisional_text_with_response_pointer() -> anyhow::Result<()> {
    let current = apply(None, &item_delta("partial")?)?;
    let event = observation(json!({"id":"Request:Mixed/Case","result":{
        "data":[{"turnId":"Turn:A","completedAtMs":1720000004567i64,"item":{"id":"Item:1","type":"agentMessage","text":"persisted"}}],
        "nextCursor":"opaque:next","backwardsCursor":null
    }}))?;
    let history = first_item(&event, Some(&scope("thread/items/list")))?;
    assert_eq!(history.evidence.json_pointer, "/result/data/0/item");
    assert_eq!(history.evidence.origin, EvidenceOrigin::RpcResponse);
    let current = apply(Some(&current), &history)?;
    assert_eq!(current.display_text[&TextChannel::Message], "persisted");
    assert_eq!(current.streamed_text[&TextChannel::Message], "partial");
    Ok(())
}

#[test]
fn completed_plan_can_differ_from_concatenated_plan_deltas() -> anyhow::Result<()> {
    let mut delta = item_delta("earlier plan")?;
    delta.update = ItemUpdate::Delta {
        channel: TextChannel::Plan,
        text: "earlier plan".into(),
    };
    let current = apply(None, &delta)?;
    let completed = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"plan","text":"revised plan"}),
    )?;
    let current = apply(Some(&current), &completed)?;
    assert_eq!(current.display_text[&TextChannel::Plan], "revised plan");
    assert_eq!(current.streamed_text[&TextChannel::Plan], "earlier plan");
    Ok(())
}

#[test]
fn contradictory_terminal_snapshot_marks_conflict_and_latest_live_snapshot_wins()
-> anyhow::Result<()> {
    let first = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"agentMessage","text":"first"}),
    )?;
    let second = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"agentMessage","text":"second"}),
    )?;
    let current = apply(None, &first)?;
    let result = reduce_item(Some(&current), &second, ReceiptApplication::Unapplied);
    assert_eq!(result.status, ReductionStatus::ConflictingSnapshot);
    let item = result.item.ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert_eq!(item.display_text[&TextChannel::Message], "second");
    assert!(item.reconciliation_required);
    Ok(())
}

#[test]
fn item_scope_same_id_in_other_thread_or_namespace_never_merges() -> anyhow::Result<()> {
    let first = item_delta("first")?;
    let current = apply(None, &first)?;
    let mut other = item_delta("second")?;
    other.key.thread_id = "Fork:other".into();
    assert_eq!(
        reduce_item(Some(&current), &other, ReceiptApplication::Unapplied).status,
        ReductionStatus::ScopeMismatch
    );
    other.key.thread_id = first.key.thread_id.clone();
    other.key.namespace_id = uuid::Uuid::new_v4();
    assert_eq!(
        reduce_item(Some(&current), &other, ReceiptApplication::Unapplied).status,
        ReductionStatus::ScopeMismatch
    );
    Ok(())
}

#[test]
fn started_snapshot_arriving_after_delta_preserves_streamed_display() -> anyhow::Result<()> {
    let current = apply(None, &item_delta("visible")?)?;
    let start = item_event(
        "item/started",
        json!({"id":"Item:1","type":"agentMessage","text":""}),
    )?;
    let current = apply(Some(&current), &start)?;
    assert_eq!(current.display_text[&TextChannel::Message], "visible");
    assert!(!current.terminal);
    Ok(())
}

#[test]
fn terminal_matching_history_enriches_missing_fields_without_false_conflict() -> anyhow::Result<()>
{
    let completed = item_event(
        "item/completed",
        json!({
            "id":"Item:1","type":"agentMessage","text":"same","future":{"kept":true}
        }),
    )?;
    let current = apply(None, &completed)?;
    let mut history = completed.clone();
    let ItemUpdate::Snapshot(snapshot) = &mut history.update else {
        anyhow::bail!("missing snapshot");
    };
    snapshot.kind = SnapshotKind::History;
    snapshot.provider_payload["phase"] = json!("final_answer");
    snapshot.provider_payload["additional"] = json!({"new":true});
    snapshot.started_at_ms = Field::Missing;
    snapshot.completed_at_ms = Field::Missing;
    let result = reduce_item(Some(&current), &history, ReceiptApplication::Unapplied);
    assert_eq!(result.status, ReductionStatus::Applied);
    let item = result.item.ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert!(item.terminal);
    assert!(!item.reconciliation_required);
    assert_eq!(item.display_text[&TextChannel::Message], "same");
    let snapshot = item
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("missing snapshot"))?;
    assert_eq!(snapshot.kind, SnapshotKind::Completed);
    assert_eq!(snapshot.started_at_ms, Field::Value(1720000000123i64));
    assert_eq!(snapshot.provider_payload["phase"], "final_answer");
    assert_eq!(snapshot.provider_payload["future"], json!({"kept":true}));
    Ok(())
}

#[test]
fn terminal_conflicting_history_content_or_timing_preserves_completion() -> anyhow::Result<()> {
    let completed = item_event(
        "item/completed",
        json!({"id":"Item:1","type":"agentMessage","text":"current"}),
    )?;
    let current = apply(None, &completed)?;
    for change_time in [false, true] {
        let mut history = completed.clone();
        let ItemUpdate::Snapshot(snapshot) = &mut history.update else {
            anyhow::bail!("missing snapshot");
        };
        snapshot.kind = SnapshotKind::History;
        if change_time {
            snapshot.completed_at_ms = Field::Value(7);
        } else {
            snapshot.provider_payload["text"] = json!("old");
            snapshot.text = Field::Value("old".into());
        }
        let result = reduce_item(Some(&current), &history, ReceiptApplication::Unapplied);
        assert_eq!(result.status, ReductionStatus::ConflictingSnapshot);
        let item = result.item.ok_or_else(|| anyhow::anyhow!("missing item"))?;
        assert_eq!(item.snapshot, current.snapshot);
        assert_eq!(item.display_text[&TextChannel::Message], "current");
        assert!(item.reconciliation_required);
    }
    Ok(())
}

#[test]
fn completed_reasoning_arrays_replace_indexed_deltas_and_remove_stale_indices() -> anyhow::Result<()>
{
    let mut current = None;
    for channel in [
        TextChannel::ReasoningSummary(0),
        TextChannel::ReasoningSummary(1),
        TextChannel::ReasoningContent(0),
    ] {
        let mut delta = item_delta("draft")?;
        delta.update = ItemUpdate::Delta {
            channel,
            text: "draft".into(),
        };
        current = Some(apply(current.as_ref(), &delta)?);
    }
    let completed = item_event(
        "item/completed",
        json!({
            "id":"Item:1","type":"reasoning","summary":["final"],"content":[]
        }),
    )?;
    let item = apply(current.as_ref(), &completed)?;
    assert_eq!(
        item.display_text[&TextChannel::ReasoningSummary(0)],
        "final"
    );
    assert!(
        !item
            .display_text
            .contains_key(&TextChannel::ReasoningSummary(1))
    );
    assert!(
        !item
            .display_text
            .contains_key(&TextChannel::ReasoningContent(0))
    );
    assert_eq!(
        item.streamed_text[&TextChannel::ReasoningSummary(0)],
        "draft"
    );
    assert_eq!(
        item.streamed_text[&TextChannel::ReasoningSummary(1)],
        "draft"
    );
    assert_eq!(
        item.streamed_text[&TextChannel::ReasoningContent(0)],
        "draft"
    );
    assert!(item.terminal);
    assert!(!item.reconciliation_required);
    Ok(())
}

#[test]
fn completed_tool_result_replaces_nested_payload_without_stale_keys() -> anyhow::Result<()> {
    let first = item_event(
        "item/completed",
        json!({
            "id":"Item:1","type":"mcpToolCall","server":"s","tool":"t","arguments":{},
            "status":"completed","result":{"structuredContent":{"answer":"A","obsolete":true}}
        }),
    )?;
    let second = item_event(
        "item/completed",
        json!({
            "id":"Item:1","type":"mcpToolCall","status":"completed",
            "result":{"structuredContent":{"answer":"B"}}
        }),
    )?;
    let current = apply(None, &first)?;
    let item = apply(Some(&current), &second)?;
    let snapshot = item
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("missing snapshot"))?;
    assert_eq!(
        snapshot.provider_payload["result"],
        json!({"structuredContent":{"answer":"B"}})
    );
    assert_eq!(snapshot.provider_payload["server"], "s");
    assert!(item.reconciliation_required);
    let mut history = second;
    let ItemUpdate::Snapshot(snapshot) = &mut history.update else {
        anyhow::bail!("missing snapshot");
    };
    snapshot.kind = SnapshotKind::History;
    let result = reduce_item(Some(&current), &history, ReceiptApplication::Unapplied);
    assert_eq!(result.status, ReductionStatus::ConflictingSnapshot);
    assert_eq!(
        result.item.as_ref().and_then(|item| item.snapshot.as_ref()),
        current.snapshot.as_ref()
    );
    Ok(())
}
