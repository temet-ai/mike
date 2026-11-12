use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use super::transcript::{LIVE_TURN, RECONNECT, SYNTHETIC_ITEMS, Transcript, connection, replay};
use super::{FrameRoute, IngestConnection, IngestOutcome};
use crate::runtime::codex::{
    ExclusionReason, Field, ItemKey, ItemUpdate, ProjectedItem, ProjectionBatch, ProjectionEffect,
    ProjectionStatus, ReceiptApplication, ReductionStatus, SignalKind, TextChannel,
    decode_observation, reduce_item,
};

struct Ingested {
    batches: Vec<ProjectionBatch>,
    excluded: Vec<(ExclusionReason, FrameRoute, Option<String>)>,
}

fn ingest(transcript: &Transcript, connection: &mut IngestConnection) -> anyhow::Result<Ingested> {
    let mut ingested = Ingested {
        batches: Vec::new(),
        excluded: Vec::new(),
    };
    for outcome in replay(transcript, connection)? {
        match outcome {
            IngestOutcome::Capture(frame) => ingested.batches.push(decode_observation(
                &frame.observation,
                frame.intake_context(),
            )),
            IngestOutcome::Excluded {
                reason,
                route,
                method,
            } => ingested.excluded.push((reason, route, method)),
        }
    }
    Ok(ingested)
}

fn load(text: &str) -> anyhow::Result<(Transcript, Ingested)> {
    let transcript = Transcript::parse(text)?;
    let mut connection = connection(&transcript)?;
    let ingested = ingest(&transcript, &mut connection)?;
    Ok((transcript, ingested))
}

fn effects(ingested: &Ingested) -> impl Iterator<Item = &ProjectionEffect> {
    ingested
        .batches
        .iter()
        .flat_map(|batch| batch.effects.iter())
}

fn reduce(
    items: &mut HashMap<ItemKey, ProjectedItem>,
    ingested: &Ingested,
) -> Vec<ReductionStatus> {
    let mut statuses = Vec::new();
    for effect in effects(ingested) {
        let ProjectionEffect::Item(observation) = effect else {
            continue;
        };
        let reduction = reduce_item(
            items.get(&observation.key),
            observation,
            ReceiptApplication::Unapplied,
        );
        statuses.push(reduction.status);
        if let Some(item) = reduction.item {
            items.insert(observation.key.clone(), item);
        }
    }
    statuses
}

fn categories(items: &HashMap<ItemKey, ProjectedItem>) -> BTreeSet<String> {
    items
        .values()
        .filter_map(|item| item.snapshot.as_ref())
        .map(|snapshot| format!("{:?}", snapshot.category))
        .collect()
}

#[test]
fn live_transcript_every_server_frame_is_captured_or_excluded_with_reason() -> anyhow::Result<()> {
    let (transcript, ingested) = load(LIVE_TURN)?;
    let received = transcript.received().count();
    assert_eq!(ingested.batches.len() + ingested.excluded.len(), received);
    let mut authentication = 0;
    for (reason, route, method) in &ingested.excluded {
        match reason {
            ExclusionReason::Authentication => {
                assert_eq!(method.as_deref(), Some("account/updated"));
                authentication += 1;
            }
            _ => {
                assert_eq!(*route, FrameRoute::Response);
                assert_eq!(*reason, ExclusionReason::UnreviewedResponse);
            }
        }
    }
    assert_eq!(authentication, 1);
    for batch in &ingested.batches {
        assert_ne!(batch.status, ProjectionStatus::Malformed, "{batch:?}");
        assert_ne!(batch.status, ProjectionStatus::Excluded, "{batch:?}");
    }
    Ok(())
}

#[test]
fn live_transcript_captures_messages_commands_output_reasoning_and_compaction() -> anyhow::Result<()>
{
    let (_, ingested) = load(LIVE_TURN)?;
    let mut items = HashMap::new();
    reduce(&mut items, &ingested);
    let found = categories(&items);
    for expected in [
        "UserMessage",
        "AssistantMessage",
        "Command",
        "Reasoning",
        "Compaction",
    ] {
        assert!(
            found.contains(expected),
            "{expected} missing from {found:?}"
        );
    }
    let message = items
        .values()
        .find(|item| {
            item.streamed_text
                .get(&TextChannel::Message)
                .is_some_and(|text| text.contains("slow-done"))
        })
        .ok_or_else(|| anyhow::anyhow!("no streamed final answer"))?;
    assert!(message.terminal);
    assert_eq!(
        message.display_text.get(&TextChannel::Message),
        message.streamed_text.get(&TextChannel::Message)
    );
    Ok(())
}

#[test]
fn long_running_command_streams_output_then_completes_with_duration() -> anyhow::Result<()> {
    let (_, ingested) = load(LIVE_TURN)?;
    let mut items = HashMap::new();
    reduce(&mut items, &ingested);
    let command = items
        .values()
        .find(|item| {
            item.key.item_id.starts_with("exec-")
                && item.streamed_text.contains_key(&TextChannel::CommandOutput)
        })
        .ok_or_else(|| anyhow::anyhow!("no command with streamed output"))?;
    assert!(command.terminal);
    assert_eq!(
        command
            .streamed_text
            .get(&TextChannel::CommandOutput)
            .map(String::as_str),
        Some("slow-done\n")
    );
    let facts = command
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.command.as_ref())
        .ok_or_else(|| anyhow::anyhow!("command facts missing"))?;
    assert!(
        matches!(facts.duration_ms, Field::Value(ms) if ms >= 3_000),
        "{facts:?}"
    );
    assert_eq!(facts.exit_code, Field::Value(0));
    Ok(())
}

#[test]
fn interrupted_turn_leaves_its_command_partial() -> anyhow::Result<()> {
    let (_, ingested) = load(LIVE_TURN)?;
    let mut items = HashMap::new();
    reduce(&mut items, &ingested);
    let partial: Vec<_> = items
        .values()
        .filter(|item| !item.terminal)
        .map(|item| item.key.item_id.clone())
        .collect();
    assert_eq!(partial.len(), 1, "{partial:?}");
    let interrupted = effects(&ingested).any(|effect| {
        matches!(effect, ProjectionEffect::Lifecycle(event)
            if event.method == "turn/completed"
                && event.status == Field::Value(Value::String("interrupted".into())))
    });
    assert!(interrupted);
    Ok(())
}

#[test]
fn fork_is_captured_as_a_thread_with_its_source() -> anyhow::Result<()> {
    let (transcript, ingested) = load(LIVE_TURN)?;
    let original = transcript.header_text("thread_id")?;
    let fork = transcript.header_text("fork_thread_id")?;
    let forked = effects(&ingested).any(|effect| {
        matches!(effect, ProjectionEffect::Lifecycle(event)
            if event.thread_id == fork
                && event.thread.as_ref().is_some_and(|facts| facts.forked_from_id == Field::Value(original.clone())))
    });
    assert!(forked);
    Ok(())
}

#[test]
fn rpc_error_is_recorded_not_dropped() -> anyhow::Result<()> {
    let (_, ingested) = load(LIVE_TURN)?;
    let errors = ingested
        .batches
        .iter()
        .filter(|batch| {
            batch.status == ProjectionStatus::Unsupported
                && batch
                    .issues
                    .iter()
                    .any(|issue| format!("{:?}", issue.kind) == "RpcError")
        })
        .count();
    assert_eq!(errors, 1);
    Ok(())
}

#[test]
fn unknown_notifications_are_kept_as_unsupported() -> anyhow::Result<()> {
    let (_, ingested) = load(LIVE_TURN)?;
    let unknown = ingested
        .batches
        .iter()
        .filter(|batch| batch.status == ProjectionStatus::Unsupported && batch.effects.is_empty())
        .count();
    assert!(unknown >= 10, "{unknown}");
    Ok(())
}

#[test]
fn usage_total_is_cumulative_and_kept_apart_from_last() -> anyhow::Result<()> {
    let (transcript, ingested) = load(LIVE_TURN)?;
    let thread = transcript.header_text("thread_id")?;
    let mut previous = 0;
    let mut updates = 0;
    for effect in effects(&ingested) {
        let ProjectionEffect::Usage(usage) = effect else {
            continue;
        };
        if usage.thread_id != thread {
            continue;
        }
        let (Field::Value(total), Field::Value(last)) = (&usage.total, &usage.last) else {
            anyhow::bail!("usage without total and last: {usage:?}");
        };
        let (Field::Value(total), Field::Value(last)) = (&total.total_tokens, &last.total_tokens)
        else {
            anyhow::bail!("usage without token totals: {usage:?}");
        };
        assert!(*total >= previous, "{total} < {previous}");
        assert!(last <= total);
        previous = *total;
        updates += 1;
    }
    assert!(updates >= 4, "{updates}");
    Ok(())
}

#[test]
fn usage_missing_is_not_zero() -> anyhow::Result<()> {
    let (_, ingested) = load(SYNTHETIC_ITEMS)?;
    let usage = effects(&ingested)
        .find_map(|effect| match effect {
            ProjectionEffect::Usage(usage) => Some(usage),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("no usage"))?;
    assert_eq!(usage.last, Field::Missing);
    assert_eq!(usage.context_window, Field::Null);
    let Field::Value(total) = &usage.total else {
        anyhow::bail!("total missing");
    };
    assert_eq!(total.total_tokens, Field::Value(0));
    Ok(())
}

#[test]
fn synthetic_edits_plans_mcp_and_custom_tools_are_captured() -> anyhow::Result<()> {
    let (transcript, ingested) = load(SYNTHETIC_ITEMS)?;
    assert_eq!(transcript.header.get("synthetic"), Some(&Value::Bool(true)));
    let mut items = HashMap::new();
    reduce(&mut items, &ingested);
    let found = categories(&items);
    for expected in ["FileEdit", "Plan", "McpTool", "CustomTool", "Unknown"] {
        assert!(
            found.contains(expected),
            "{expected} missing from {found:?}"
        );
    }
    let signals: BTreeSet<_> = effects(&ingested)
        .filter_map(|effect| match effect {
            ProjectionEffect::Signal(signal) => Some(format!("{:?}", signal.kind)),
            _ => None,
        })
        .collect();
    for kind in [
        SignalKind::TurnPlan,
        SignalKind::McpProgress,
        SignalKind::Error,
    ] {
        assert!(signals.contains(&format!("{kind:?}")), "{signals:?}");
    }
    let custom = items
        .values()
        .find(|item| item.key.item_id == "dyn-1")
        .and_then(|item| item.snapshot.as_ref()?.tool.clone())
        .ok_or_else(|| anyhow::anyhow!("custom tool facts missing"))?;
    assert_eq!(custom.success, Field::Value(false));
    Ok(())
}

#[test]
fn reconnect_history_deduplicates_by_provider_ids() -> anyhow::Result<()> {
    let live = Transcript::parse(LIVE_TURN)?;
    let mut connection = connection(&live)?;
    let first = ingest(&live, &mut connection)?;
    let mut items = HashMap::new();
    reduce(&mut items, &first);
    let live_keys: BTreeSet<_> = items.keys().cloned().collect();
    let epoch = connection.epoch();
    assert_ne!(connection.reconnect(), epoch);
    let second = ingest(&Transcript::parse(RECONNECT)?, &mut connection)?;
    let history: BTreeSet<_> = effects(&second)
        .filter_map(|effect| match effect {
            ProjectionEffect::Item(item) if matches!(item.update, ItemUpdate::Snapshot(_)) => {
                Some(item.key.clone())
            }
            _ => None,
        })
        .collect();
    assert!(!history.is_empty());
    let statuses = reduce(&mut items, &second);
    assert!(
        statuses
            .iter()
            .all(|status| *status == ReductionStatus::Applied),
        "{statuses:?}"
    );
    let all: BTreeSet<_> = live_keys.union(&history).cloned().collect();
    assert_eq!(items.len(), all.len());
    assert!(
        history.iter().all(|key| live_keys.contains(key)),
        "history found items live missed"
    );
    Ok(())
}
