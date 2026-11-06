use serde_json::Value;

use super::items::{ItemKey, ItemObservation, ItemSnapshot, ItemUpdate, SnapshotKind, TextChannel};
use super::protocol::{Evidence, IssueKind, ProjectionIssue, RpcScope, identifier};

pub(super) fn live_snapshot(
    params: &Value,
    kind: SnapshotKind,
    evidence: Evidence,
) -> Result<ItemObservation, ProjectionIssue> {
    let thread = identifier(params, "/threadId").map_err(|issue| issue.prefixed("/params"))?;
    let turn = identifier(params, "/turnId").map_err(|issue| issue.prefixed("/params"))?;
    let item = params
        .get("item")
        .ok_or_else(|| ProjectionIssue::new(IssueKind::MissingField, "/params/item"))?;
    snapshot(item, params, &thread, &turn, kind, evidence)
}

pub(super) fn history_snapshot(
    entry: &Value,
    scope: &RpcScope,
    evidence: Evidence,
) -> Result<ItemObservation, ProjectionIssue> {
    let thread = scope
        .thread_id
        .as_ref()
        .ok_or_else(|| ProjectionIssue::new(IssueKind::MissingField, "/request/threadId"))?;
    identifier(&serde_json::json!({"threadId": thread}), "/threadId")
        .map_err(|issue| issue.prefixed("/request"))?;
    let turn = identifier(entry, "/turnId")?;
    if scope
        .turn_id
        .as_ref()
        .is_some_and(|requested| *requested != turn)
    {
        return Err(ProjectionIssue::new(
            IssueKind::ConflictingAssociation,
            "/turnId",
        ));
    }
    let item = entry
        .get("item")
        .ok_or_else(|| ProjectionIssue::new(IssueKind::MissingField, "/item"))?;
    snapshot(item, entry, thread, &turn, SnapshotKind::History, evidence)
}

fn snapshot(
    item: &Value,
    timing: &Value,
    thread: &str,
    turn: &str,
    kind: SnapshotKind,
    evidence: Evidence,
) -> Result<ItemObservation, ProjectionIssue> {
    let item_id =
        identifier(item, "/id").map_err(|issue| issue.prefixed(&evidence.json_pointer))?;
    identifier(item, "/type").map_err(|issue| issue.prefixed(&evidence.json_pointer))?;
    Ok(ItemObservation {
        key: ItemKey {
            namespace_id: evidence.receipt.namespace_id,
            thread_id: thread.to_owned(),
            turn_id: turn.to_owned(),
            item_id,
        },
        evidence,
        update: ItemUpdate::Snapshot(Box::new(ItemSnapshot::new(item.clone(), kind, timing))),
    })
}

pub(super) fn delta_channel(
    method: &str,
    params: &Value,
) -> Result<Option<TextChannel>, ProjectionIssue> {
    let channel = match method {
        "item/agentMessage/delta" => TextChannel::Message,
        "item/plan/delta" => TextChannel::Plan,
        "item/commandExecution/outputDelta" => TextChannel::CommandOutput,
        "item/fileChange/outputDelta" => TextChannel::FileOutput,
        "item/reasoning/textDelta" => TextChannel::ReasoningContent(index(params, "contentIndex")?),
        "item/reasoning/summaryTextDelta" => {
            TextChannel::ReasoningSummary(index(params, "summaryIndex")?)
        }
        _ => return Ok(None),
    };
    Ok(Some(channel))
}

fn index(params: &Value, key: &str) -> Result<i64, ProjectionIssue> {
    params
        .get(key)
        .and_then(Value::as_i64)
        .filter(|index| *index >= 0)
        .ok_or_else(|| ProjectionIssue::new(IssueKind::InvalidField, format!("/params/{key}")))
}

pub(super) fn delta(
    params: &Value,
    channel: TextChannel,
    evidence: Evidence,
) -> Result<ItemObservation, ProjectionIssue> {
    let text = params
        .get("delta")
        .and_then(Value::as_str)
        .ok_or_else(|| ProjectionIssue::new(IssueKind::InvalidField, "/params/delta"))?;
    Ok(ItemObservation {
        key: ItemKey {
            namespace_id: evidence.receipt.namespace_id,
            thread_id: identifier(params, "/threadId")
                .map_err(|issue| issue.prefixed("/params"))?,
            turn_id: identifier(params, "/turnId").map_err(|issue| issue.prefixed("/params"))?,
            item_id: identifier(params, "/itemId").map_err(|issue| issue.prefixed("/params"))?,
        },
        evidence,
        update: ItemUpdate::Delta {
            channel,
            text: text.to_owned(),
        },
    })
}
