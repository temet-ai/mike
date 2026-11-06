use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::protocol::{Evidence, Field, field};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemKey {
    pub namespace_id: Uuid,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemCategory {
    UserMessage,
    AssistantMessage,
    Command,
    McpTool,
    CustomTool,
    ToolOutput,
    FileEdit,
    Plan,
    Reasoning,
    Compaction,
    Collaboration,
    Unknown,
}

impl ItemCategory {
    pub(super) fn from_provider_type(value: &str) -> Self {
        match value {
            "userMessage" => Self::UserMessage,
            "agentMessage" => Self::AssistantMessage,
            "commandExecution" => Self::Command,
            "mcpToolCall" => Self::McpTool,
            "dynamicToolCall" => Self::CustomTool,
            "functionCallOutput" => Self::ToolOutput,
            "fileChange" => Self::FileEdit,
            "plan" => Self::Plan,
            "reasoning" => Self::Reasoning,
            "contextCompaction" => Self::Compaction,
            "collabAgentToolCall" | "subAgentActivity" => Self::Collaboration,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandFacts {
    pub command: Field<String>,
    pub cwd: Field<String>,
    pub process_id: Field<String>,
    pub output: Field<String>,
    pub exit_code: Field<i64>,
    pub duration_ms: Field<i64>,
    pub actions: Field<Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFacts {
    pub server: Field<String>,
    pub namespace: Field<String>,
    pub tool: Field<String>,
    pub arguments: Field<Value>,
    pub result: Field<Value>,
    pub error: Field<Value>,
    pub success: Field<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotKind {
    Started,
    Completed,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSnapshot {
    pub kind: SnapshotKind,
    pub provider_type: String,
    pub category: ItemCategory,
    pub status: Field<String>,
    pub text: Field<String>,
    pub content: Field<Value>,
    pub command: Option<CommandFacts>,
    pub tool: Option<ToolFacts>,
    pub file_changes: Field<Vec<Value>>,
    pub started_at_ms: Field<i64>,
    pub completed_at_ms: Field<i64>,
    pub provider_payload: Value,
}

impl ItemSnapshot {
    pub(super) fn new(value: Value, kind: SnapshotKind, timing: &Value) -> Self {
        let provider_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let category = ItemCategory::from_provider_type(provider_type);
        Self {
            kind,
            provider_type: provider_type.to_owned(),
            category,
            status: field(&value, "status"),
            text: field(&value, "text"),
            content: field(&value, "content"),
            command: command_facts(&value, category),
            tool: tool_facts(&value, category),
            file_changes: field(&value, "changes"),
            started_at_ms: field(timing, "startedAtMs"),
            completed_at_ms: field(timing, "completedAtMs"),
            provider_payload: value,
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.kind == SnapshotKind::Completed
            || matches!(self.completed_at_ms, Field::Value(_))
            || self.status.value().is_some_and(|status| {
                matches!(
                    status.as_str(),
                    "completed" | "failed" | "declined" | "interrupted"
                )
            })
    }
}

fn command_facts(value: &Value, category: ItemCategory) -> Option<CommandFacts> {
    (category == ItemCategory::Command).then(|| CommandFacts {
        command: field(value, "command"),
        cwd: field(value, "cwd"),
        process_id: field(value, "processId"),
        output: field(value, "aggregatedOutput"),
        exit_code: field(value, "exitCode"),
        duration_ms: field(value, "durationMs"),
        actions: field(value, "commandActions"),
    })
}

fn tool_facts(value: &Value, category: ItemCategory) -> Option<ToolFacts> {
    matches!(
        category,
        ItemCategory::McpTool | ItemCategory::CustomTool | ItemCategory::ToolOutput
    )
    .then(|| ToolFacts {
        server: field(value, "server"),
        namespace: field(value, "namespace"),
        tool: field(
            value,
            if category == ItemCategory::ToolOutput {
                "name"
            } else {
                "tool"
            },
        ),
        arguments: field(value, "arguments"),
        result: field(
            value,
            if category == ItemCategory::ToolOutput {
                "output"
            } else {
                "result"
            },
        ),
        error: field(value, "error"),
        success: field(value, "success"),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TextChannel {
    Message,
    Plan,
    CommandOutput,
    FileOutput,
    ReasoningContent(i64),
    ReasoningSummary(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemUpdate {
    Snapshot(Box<ItemSnapshot>),
    Delta { channel: TextChannel, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemObservation {
    pub key: ItemKey,
    pub evidence: Evidence,
    pub update: ItemUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedItem {
    pub key: ItemKey,
    pub snapshot: Option<ItemSnapshot>,
    pub display_text: BTreeMap<TextChannel, String>,
    pub streamed_text: BTreeMap<TextChannel, String>,
    pub terminal: bool,
    pub reconciliation_required: bool,
    pub snapshot_evidence: Option<Evidence>,
    pub first_evidence: Evidence,
    pub latest_evidence: Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptApplication {
    Unapplied,
    AlreadyApplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReductionStatus {
    Applied,
    AlreadyApplied,
    ScopeMismatch,
    PreservedTerminal,
    ConflictingSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemReduction {
    pub item: Option<ProjectedItem>,
    pub status: ReductionStatus,
}

pub fn reduce_item(
    current: Option<&ProjectedItem>,
    observation: &ItemObservation,
    receipt: ReceiptApplication,
) -> ItemReduction {
    if receipt == ReceiptApplication::AlreadyApplied {
        return ItemReduction {
            item: current.cloned(),
            status: ReductionStatus::AlreadyApplied,
        };
    }
    if current.is_some_and(|item| item.key != observation.key) {
        return ItemReduction {
            item: current.cloned(),
            status: ReductionStatus::ScopeMismatch,
        };
    }
    let mut item = match current {
        Some(item) => item.clone(),
        None => ProjectedItem {
            key: observation.key.clone(),
            snapshot: None,
            display_text: BTreeMap::new(),
            streamed_text: BTreeMap::new(),
            terminal: false,
            reconciliation_required: false,
            snapshot_evidence: None,
            first_evidence: observation.evidence.clone(),
            latest_evidence: observation.evidence.clone(),
        },
    };
    item.latest_evidence = observation.evidence.clone();
    let status = match &observation.update {
        ItemUpdate::Snapshot(snapshot) => {
            apply_snapshot(&mut item, snapshot, &observation.evidence)
        }
        ItemUpdate::Delta { channel, text } => apply_delta(&mut item, channel, text),
    };
    ItemReduction {
        item: Some(item),
        status,
    }
}

fn apply_delta(item: &mut ProjectedItem, channel: &TextChannel, text: &str) -> ReductionStatus {
    item.streamed_text
        .entry(channel.clone())
        .or_default()
        .push_str(text);
    if item.terminal {
        item.reconciliation_required = true;
        return ReductionStatus::PreservedTerminal;
    }
    item.display_text
        .entry(channel.clone())
        .or_default()
        .push_str(text);
    ReductionStatus::Applied
}

fn apply_snapshot(
    item: &mut ProjectedItem,
    incoming: &ItemSnapshot,
    evidence: &Evidence,
) -> ReductionStatus {
    if item.terminal && incoming.kind == SnapshotKind::History {
        return apply_terminal_history(item, incoming, evidence);
    }
    if item.terminal && !incoming.is_terminal() {
        return ReductionStatus::PreservedTerminal;
    }
    let conflict = item.terminal && snapshot_conflicts(item.snapshot.as_ref(), incoming);
    item.reconciliation_required |= conflict;
    let snapshot = merge_snapshot(item.snapshot.as_ref(), incoming);
    replace_snapshot_text(item, incoming);
    item.terminal |= snapshot.is_terminal();
    item.snapshot = Some(snapshot);
    item.snapshot_evidence = Some(evidence.clone());
    if conflict {
        ReductionStatus::ConflictingSnapshot
    } else {
        ReductionStatus::Applied
    }
}

fn apply_terminal_history(
    item: &mut ProjectedItem,
    incoming: &ItemSnapshot,
    evidence: &Evidence,
) -> ReductionStatus {
    if snapshot_conflicts(item.snapshot.as_ref(), incoming) {
        item.reconciliation_required = true;
        return ReductionStatus::ConflictingSnapshot;
    }
    let mut merged = merge_snapshot(item.snapshot.as_ref(), incoming);
    merged.kind = item
        .snapshot
        .as_ref()
        .map_or(SnapshotKind::Completed, |snapshot| snapshot.kind);
    replace_snapshot_text(item, incoming);
    item.snapshot = Some(merged);
    item.snapshot_evidence = Some(evidence.clone());
    ReductionStatus::Applied
}

fn snapshot_conflicts(current: Option<&ItemSnapshot>, incoming: &ItemSnapshot) -> bool {
    let Some(current) = current else {
        return false;
    };
    payload_conflicts(&current.provider_payload, &incoming.provider_payload)
        || timing_conflicts(&current.started_at_ms, &incoming.started_at_ms)
        || timing_conflicts(&current.completed_at_ms, &incoming.completed_at_ms)
}

fn payload_conflicts(current: &Value, incoming: &Value) -> bool {
    match (current.as_object(), incoming.as_object()) {
        (Some(current), Some(incoming)) => incoming
            .iter()
            .any(|(key, value)| current.get(key).is_some_and(|old| old != value)),
        _ => current != incoming,
    }
}

fn timing_conflicts(current: &Field<i64>, incoming: &Field<i64>) -> bool {
    !matches!(current, Field::Missing) && !matches!(incoming, Field::Missing) && current != incoming
}

fn merge_payload(current: &mut Value, incoming: &Value) {
    match (current.as_object_mut(), incoming.as_object()) {
        (Some(current), Some(incoming)) => current.extend(incoming.clone()),
        _ => *current = incoming.clone(),
    }
}

fn merge_snapshot(current: Option<&ItemSnapshot>, incoming: &ItemSnapshot) -> ItemSnapshot {
    let Some(current) = current else {
        return incoming.clone();
    };
    let mut payload = current.provider_payload.clone();
    merge_payload(&mut payload, &incoming.provider_payload);
    let mut merged = ItemSnapshot::new(payload, incoming.kind, &Value::Null);
    merged.started_at_ms = retain_missing(&current.started_at_ms, &incoming.started_at_ms);
    merged.completed_at_ms = retain_missing(&current.completed_at_ms, &incoming.completed_at_ms);
    merged
}

fn retain_missing<T: Clone>(old: &Field<T>, new: &Field<T>) -> Field<T> {
    if matches!(new, Field::Missing) {
        old.clone()
    } else {
        new.clone()
    }
}

fn replace_snapshot_text(item: &mut ProjectedItem, snapshot: &ItemSnapshot) {
    if snapshot.category == ItemCategory::Reasoning {
        replace_reasoning_text(item, snapshot, true);
        replace_reasoning_text(item, snapshot, false);
    }
    let channel = match snapshot.category {
        ItemCategory::AssistantMessage => Some(TextChannel::Message),
        ItemCategory::Plan => Some(TextChannel::Plan),
        _ => None,
    };
    if let Some(channel) = channel {
        replace_text_field(item, channel, &snapshot.text, snapshot.kind);
    }
    if let Some(command) = &snapshot.command {
        replace_text_field(
            item,
            TextChannel::CommandOutput,
            &command.output,
            snapshot.kind,
        );
    }
}

fn replace_text_field(
    item: &mut ProjectedItem,
    channel: TextChannel,
    text: &Field<String>,
    kind: SnapshotKind,
) {
    if kind == SnapshotKind::Started && item.streamed_text.contains_key(&channel) {
        return;
    }
    match text {
        Field::Value(text) => {
            item.display_text.insert(channel, text.clone());
        }
        Field::Null => {
            item.display_text.remove(&channel);
        }
        Field::Invalid(_) => {
            item.reconciliation_required = true;
        }
        Field::Missing => {}
    }
}

fn replace_reasoning_text(item: &mut ProjectedItem, snapshot: &ItemSnapshot, summary: bool) {
    let key = if summary { "summary" } else { "content" };
    let values = match field::<Vec<String>>(&snapshot.provider_payload, key) {
        Field::Value(values) => values,
        Field::Null => Vec::new(),
        Field::Missing => return,
        Field::Invalid(_) => {
            item.reconciliation_required = true;
            return;
        }
    };
    if snapshot.kind == SnapshotKind::Started
        && item
            .streamed_text
            .keys()
            .any(|channel| reasoning_channel(channel, summary))
    {
        return;
    }
    item.display_text
        .retain(|channel, _| !reasoning_channel(channel, summary));
    for (index, text) in values.into_iter().enumerate() {
        let Ok(index) = i64::try_from(index) else {
            item.reconciliation_required = true;
            continue;
        };
        let channel = if summary {
            TextChannel::ReasoningSummary(index)
        } else {
            TextChannel::ReasoningContent(index)
        };
        item.display_text.insert(channel, text);
    }
}

fn reasoning_channel(channel: &TextChannel, summary: bool) -> bool {
    matches!(
        (channel, summary),
        (TextChannel::ReasoningSummary(_), true) | (TextChannel::ReasoningContent(_), false)
    )
}
