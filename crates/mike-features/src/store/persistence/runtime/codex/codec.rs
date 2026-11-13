use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::runtime::codex::{Evidence, ItemKey, ItemSnapshot, ProjectedItem, TextChannel};
use crate::store::persistence::PersistError;

pub const STATE_CODEC_VERSION: i32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProjectedItemV1 {
    state_codec_version: i32,
    key: ItemKey,
    #[serde(deserialize_with = "required_optional")]
    snapshot: Option<ItemSnapshot>,
    display_text: Vec<StoredTextChannel>,
    streamed_text: Vec<StoredTextChannel>,
    terminal: bool,
    reconciliation_required: bool,
    #[serde(deserialize_with = "required_optional")]
    snapshot_evidence: Option<Evidence>,
    first_evidence: Evidence,
    latest_evidence: Evidence,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTextChannel {
    tag: ChannelTag,
    #[serde(deserialize_with = "required_optional")]
    index: Option<i64>,
    text: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ChannelTag {
    Message,
    Plan,
    CommandOutput,
    FileOutput,
    ReasoningContent,
    ReasoningSummary,
}

pub fn encode_item(item: &ProjectedItem) -> Result<String, PersistError> {
    validate_item(item)?;
    Ok(serde_json::to_string(&StoredProjectedItemV1::from(item))?)
}

pub fn decode_item(document: &str, expected_key: &ItemKey) -> Result<ProjectedItem, PersistError> {
    let stored: StoredProjectedItemV1 = serde_json::from_str(document)?;
    if stored.state_codec_version != STATE_CODEC_VERSION {
        return Err(invalid_state("unsupported item state codec version"));
    }
    if &stored.key != expected_key {
        return Err(invalid_state("stored item identity does not match its key"));
    }
    let item = stored.into_item()?;
    validate_item(&item)?;
    Ok(item)
}

impl From<&ProjectedItem> for StoredProjectedItemV1 {
    fn from(item: &ProjectedItem) -> Self {
        Self {
            state_codec_version: STATE_CODEC_VERSION,
            key: item.key.clone(),
            snapshot: item.snapshot.clone(),
            display_text: encode_channels(&item.display_text),
            streamed_text: encode_channels(&item.streamed_text),
            terminal: item.terminal,
            reconciliation_required: item.reconciliation_required,
            snapshot_evidence: item.snapshot_evidence.clone(),
            first_evidence: item.first_evidence.clone(),
            latest_evidence: item.latest_evidence.clone(),
        }
    }
}

impl StoredProjectedItemV1 {
    fn into_item(self) -> Result<ProjectedItem, PersistError> {
        Ok(ProjectedItem {
            key: self.key,
            snapshot: self.snapshot,
            display_text: decode_channels(self.display_text)?,
            streamed_text: decode_channels(self.streamed_text)?,
            terminal: self.terminal,
            reconciliation_required: self.reconciliation_required,
            snapshot_evidence: self.snapshot_evidence,
            first_evidence: self.first_evidence,
            latest_evidence: self.latest_evidence,
        })
    }
}

fn encode_channels(channels: &BTreeMap<TextChannel, String>) -> Vec<StoredTextChannel> {
    channels
        .iter()
        .map(|(channel, text)| {
            let (tag, index) = match channel {
                TextChannel::Message => (ChannelTag::Message, None),
                TextChannel::Plan => (ChannelTag::Plan, None),
                TextChannel::CommandOutput => (ChannelTag::CommandOutput, None),
                TextChannel::FileOutput => (ChannelTag::FileOutput, None),
                TextChannel::ReasoningContent(index) => {
                    (ChannelTag::ReasoningContent, Some(*index))
                }
                TextChannel::ReasoningSummary(index) => {
                    (ChannelTag::ReasoningSummary, Some(*index))
                }
            };
            StoredTextChannel {
                tag,
                index,
                text: text.clone(),
            }
        })
        .collect()
}

fn decode_channels(
    channels: Vec<StoredTextChannel>,
) -> Result<BTreeMap<TextChannel, String>, PersistError> {
    let mut result = BTreeMap::new();
    for channel in channels {
        let key = decode_channel(channel.tag, channel.index)?;
        if result.insert(key, channel.text).is_some() {
            return Err(invalid_state("duplicate stored item text channel"));
        }
    }
    Ok(result)
}

fn decode_channel(tag: ChannelTag, index: Option<i64>) -> Result<TextChannel, PersistError> {
    match (tag, index) {
        (ChannelTag::Message, None) => Ok(TextChannel::Message),
        (ChannelTag::Plan, None) => Ok(TextChannel::Plan),
        (ChannelTag::CommandOutput, None) => Ok(TextChannel::CommandOutput),
        (ChannelTag::FileOutput, None) => Ok(TextChannel::FileOutput),
        (ChannelTag::ReasoningContent, Some(index)) if index >= 0 => {
            Ok(TextChannel::ReasoningContent(index))
        }
        (ChannelTag::ReasoningSummary, Some(index)) if index >= 0 => {
            Ok(TextChannel::ReasoningSummary(index))
        }
        (ChannelTag::ReasoningContent | ChannelTag::ReasoningSummary, _) => Err(invalid_state(
            "reasoning channel requires a nonnegative index",
        )),
        _ => Err(invalid_state("non-indexed text channel carries an index")),
    }
}

fn validate_item(item: &ProjectedItem) -> Result<(), PersistError> {
    for evidence in [&item.first_evidence, &item.latest_evidence]
        .into_iter()
        .chain(item.snapshot_evidence.as_ref())
    {
        validate_evidence(evidence, &item.key)?;
    }
    for channel in item.display_text.keys().chain(item.streamed_text.keys()) {
        if matches!(
            channel,
            TextChannel::ReasoningContent(index) | TextChannel::ReasoningSummary(index) if *index < 0
        ) {
            return Err(invalid_state(
                "reasoning channel requires a nonnegative index",
            ));
        }
    }
    Ok(())
}

fn validate_evidence(evidence: &Evidence, key: &ItemKey) -> Result<(), PersistError> {
    if evidence.receipt.namespace_id.is_nil() || evidence.receipt.ingestion_id.is_nil() {
        return Err(invalid_state(
            "item evidence contains a nil receipt identifier",
        ));
    }
    if evidence.receipt.namespace_id != key.namespace_id {
        return Err(invalid_state(
            "item evidence receipt namespace does not match its key",
        ));
    }
    Ok(())
}

fn invalid_state(message: &str) -> PersistError {
    PersistError::DataInvariant(message.to_owned())
}

fn required_optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
