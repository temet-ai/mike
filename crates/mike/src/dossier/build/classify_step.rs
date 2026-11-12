use std::collections::HashMap;

use crate::store::persistence::prompt_classification::{ReclassifyRow, capture_rule_code};
use crate::store::persistence::prompt_stream::StreamRow;
use crate::store::persistence::turn_facts::{OpenedByRow, PromptToClassify, TurnRecord};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::wire::genuine::repeat::repeat_key;
use crate::wire::genuine::stream::StreamMessage;
use crate::wire::genuine::{
    CLASSIFIER_VERSION, CaptureRule, DispositionClass, PromptClass, PromptInput, PromptKind,
    classify_prompt,
};

fn reclassify_row(prompt: &PromptToClassify) -> ReclassifyRow {
    let class = classify_prompt(&PromptInput {
        text: &prompt.human_prompt,
        last_user_message: Some(&prompt.last_user_message),
        author: &prompt.author,
        has_agent_id: prompt.has_agent_id,
    });
    ReclassifyRow {
        request_id: prompt.request_id,
        session_id: prompt.session_id,
        kind: class.kind.stored_code().to_string(),
        rule: class.rule.map(|rule| capture_rule_code(rule).to_string()),
        repeat_key: repeat_key(&prompt.human_prompt),
        version: CLASSIFIER_VERSION.to_string(),
    }
}

pub async fn ensure_classifications(
    persist: &PersistHandle,
    session_ids: &[i64],
) -> Result<u64, PersistError> {
    let pending = persist
        .load_prompts_needing_classification(session_ids, CLASSIFIER_VERSION)
        .await?;
    if pending.is_empty() {
        return Ok(0);
    }
    let rows: Vec<ReclassifyRow> = pending.iter().map(reclassify_row).collect();
    let outcome = persist.reclassify_batch(&rows).await?;
    Ok(outcome.updated + outcome.inserted)
}

fn capture_rule_of(code: Option<&str>) -> Option<CaptureRule> {
    match code {
        Some("a") => Some(CaptureRule::A),
        Some("c") => Some(CaptureRule::C),
        Some("envelope") => Some(CaptureRule::Envelope),
        _ => None,
    }
}

pub fn stream_message(row: &StreamRow) -> Option<StreamMessage> {
    let kind = PromptKind::from_stored_code(row.kind_code.as_deref()?)?;
    Some(StreamMessage {
        request_id: row.request_id,
        at: row.created_at,
        capture: PromptClass {
            kind,
            rule: capture_rule_of(row.rule.as_deref()),
        },
        repeat_key: row.repeat_key.clone(),
        turn_id: row.turn_id,
    })
}

pub fn group_streams(rows: &[StreamRow]) -> HashMap<i64, Vec<StreamMessage>> {
    let mut grouped: HashMap<i64, Vec<StreamMessage>> = HashMap::new();
    for row in rows {
        if let Some(message) = stream_message(row) {
            grouped.entry(row.session_id).or_default().push(message);
        }
    }
    grouped
}

pub fn opened_by_for(stream: &[StreamMessage], start_request_id: i64) -> &'static str {
    let class = stream
        .binary_search_by_key(&start_request_id, |message| message.request_id)
        .ok()
        .map(|index| stream[index].capture.kind.class());
    match class {
        Some(DispositionClass::Operator) => "operator",
        Some(DispositionClass::LaneForeign) => "agent",
        _ => "machine",
    }
}

pub fn derive_opened_by(
    streams: &HashMap<i64, Vec<StreamMessage>>,
    turns: &mut [TurnRecord],
) -> Vec<OpenedByRow> {
    let empty = Vec::new();
    turns
        .iter_mut()
        .filter(|turn| turn.opened_by.is_none())
        .map(|turn| {
            let stream = streams.get(&turn.session_id).unwrap_or(&empty);
            let opened_by = opened_by_for(stream, turn.start_request_id);
            turn.opened_by = Some(opened_by.to_string());
            OpenedByRow {
                turn_id: turn.turn_id,
                opened_by: opened_by.to_string(),
            }
        })
        .collect()
}
