use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::types::{AsOf, CorrectionItem, DossierInputs, Exclusion, ReactionItem};
use crate::dossier::build::classify_step::stream_message;
use crate::dossier::build::resolve::rule_code;
use crate::dossier::genuine::PromptKind;
use crate::dossier::genuine::stream::{
    Effective, StreamMessage, TurnResolution, effective_kinds, resolve_turns,
};
use crate::dossier::render::cut::slice_chars;
use crate::dossier::render::sentences::sentence_containing;
use crate::store::persistence::dossier_entries::DossierEntryRow;
use crate::store::persistence::dossier_view::{DossierStreamRow, DossierTurnRow, SessionHeaderRow};
use crate::store::persistence::prompt_stream::StreamRow;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextWithTime {
    pub text: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionView {
    #[serde(flatten)]
    pub header: SessionHeaderRow,
    pub instructions: Option<TextWithTime>,
    pub goal: Option<TextWithTime>,
    pub session_class: Option<String>,
    pub session_class_rule: Option<String>,
    pub facts_built_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StreamItem {
    #[serde(flatten)]
    pub row: DossierStreamRow,
    pub effective_kind: Option<&'static str>,
    pub effective_rule: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NextItem {
    pub state: &'static str,
    pub request_id: Option<i64>,
    pub turn_id: Option<i64>,
    pub at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TurnItem {
    #[serde(flatten)]
    pub row: DossierTurnRow,
    pub prompt_kind_as_of: Option<&'static str>,
    pub next: NextItem,
    pub reply_window_segment_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AskLedgerRef {
    pub entry_id: i64,
    pub kind_at: String,
    pub kind_source: String,
    pub keyword_kind: Option<String>,
    pub operator_turn_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AskItem {
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub text: String,
    pub turn_id: Option<i64>,
    pub ledger: Option<AskLedgerRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryItem {
    #[serde(flatten)]
    pub entry: DossierEntryRow,
    pub status_at: String,
    pub text_span: Option<String>,
    pub sentence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpenTurnText {
    pub turn_id: i64,
    pub text: String,
    pub spans: Vec<(String, usize, usize)>,
    pub max_request_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DossierView {
    pub as_of: AsOf,
    pub session: SessionView,
    pub stream: Vec<StreamItem>,
    pub turns: Vec<TurnItem>,
    pub asks: Vec<AskItem>,
    pub directives: Vec<EntryItem>,
    pub promises: Vec<EntryItem>,
    pub open_turn: Option<OpenTurnText>,
    pub corrections: Vec<CorrectionItem>,
    pub reactions: Vec<ReactionItem>,
    pub exclusions: Vec<Exclusion>,
}

pub fn messages(inputs: &DossierInputs) -> Vec<StreamMessage> {
    inputs
        .stream
        .iter()
        .filter_map(|row| {
            stream_message(&StreamRow {
                session_id: inputs.session.session_db_id,
                request_id: row.request_id,
                created_at: row.at,
                turn_id: row.turn_id,
                kind_code: row.kind_code.clone(),
                rule: row.rule.clone(),
                repeat_key: row.repeat_key.clone(),
                classifier_version: row.classifier_version.clone(),
                text: None,
                author: None,
                has_agent_id: false,
            })
        })
        .collect()
}

pub fn effective_by_request(inputs: &DossierInputs) -> HashMap<i64, Effective> {
    let stream = messages(inputs);
    let effective = effective_kinds(&stream, inputs.as_of.at);
    stream
        .iter()
        .zip(effective)
        .filter_map(|(message, effective)| effective.map(|e| (message.request_id, e)))
        .collect()
}

pub fn resolutions(inputs: &DossierInputs) -> HashMap<i64, TurnResolution> {
    resolve_turns(&messages(inputs), inputs.as_of.at)
        .into_iter()
        .map(|r| (r.turn_id, r))
        .collect()
}

pub fn genuine_stream(inputs: &DossierInputs) -> Vec<&DossierStreamRow> {
    let effective = effective_by_request(inputs);
    inputs
        .stream
        .iter()
        .filter(|row| {
            effective
                .get(&row.request_id)
                .is_some_and(|e| e.kind == PromptKind::Genuine)
        })
        .collect()
}

pub fn source_request_id(source_ref: &str) -> Option<i64> {
    source_ref.strip_prefix("requests.requests:")?.parse().ok()
}

pub fn span_of(entry: &DossierEntryRow) -> Option<(usize, usize)> {
    let span = entry.data.get("span")?.as_array()?;
    let start = usize::try_from(span.first()?.as_i64()?).ok()?;
    let end = usize::try_from(span.get(1)?.as_i64()?).ok()?;
    Some((start, end))
}

fn directive_item(entry: &DossierEntryRow, prompts: &HashMap<i64, &str>) -> EntryItem {
    let prompt = source_request_id(&entry.source_ref).and_then(|id| prompts.get(&id).copied());
    let span = span_of(entry);
    EntryItem {
        status_at: entry.status.clone(),
        text_span: prompt
            .zip(span)
            .map(|(p, (s, e))| slice_chars(p, s, e).to_owned()),
        sentence: prompt
            .zip(span)
            .and_then(|(p, (s, _))| sentence_containing(p, s)),
        entry: entry.clone(),
    }
}

fn promise_item(entry: &DossierEntryRow, turn_texts: &HashMap<i64, &str>) -> EntryItem {
    let text = entry
        .operator_turn_id
        .and_then(|id| turn_texts.get(&id).copied());
    EntryItem {
        status_at: entry.status.clone(),
        text_span: text
            .zip(span_of(entry))
            .map(|(t, (s, e))| slice_chars(t, s, e).to_owned()),
        sentence: None,
        entry: entry.clone(),
    }
}

fn next_item(resolution: Option<&TurnResolution>) -> NextItem {
    match resolution.and_then(|r| r.next_genuine.as_ref()) {
        Some(next) => NextItem {
            state: "resolved",
            request_id: Some(next.request_id),
            turn_id: next.turn_id,
            at: Some(next.at),
        },
        None => NextItem {
            state: "pending",
            request_id: None,
            turn_id: None,
            at: None,
        },
    }
}

fn session_view(inputs: &DossierInputs) -> SessionView {
    let last_facts = inputs
        .turns
        .iter()
        .rev()
        .find(|t| t.session_class.is_some());
    SessionView {
        header: inputs.session.clone(),
        instructions: inputs.goals.as_ref().and_then(|g| {
            g.instructions.clone().map(|text| TextWithTime {
                text,
                updated_at: g.updated_at,
            })
        }),
        goal: inputs.goals.as_ref().map(|g| TextWithTime {
            text: g.goal.clone(),
            updated_at: g.updated_at,
        }),
        session_class: last_facts.and_then(|t| t.session_class.clone()),
        session_class_rule: last_facts.and_then(|t| t.session_class_rule.clone()),
        facts_built_at: last_facts.and_then(|t| t.built_at),
    }
}

fn stream_items(inputs: &DossierInputs) -> Vec<StreamItem> {
    let effective = effective_by_request(inputs);
    inputs
        .stream
        .iter()
        .map(|row| {
            let e = effective.get(&row.request_id);
            StreamItem {
                row: row.clone(),
                effective_kind: e.map(|e| e.kind.j2_code()),
                effective_rule: e.and_then(|e| e.rule).map(rule_code),
            }
        })
        .collect()
}

fn turn_items(inputs: &DossierInputs) -> Vec<TurnItem> {
    let effective = effective_by_request(inputs);
    let resolved = resolutions(inputs);
    inputs
        .turns
        .iter()
        .map(|turn| {
            let resolution = resolved.get(&turn.turn_id);
            TurnItem {
                row: turn.clone(),
                prompt_kind_as_of: effective
                    .get(&turn.start_request_id)
                    .map(|e| e.kind.j2_code()),
                next: next_item(resolution),
                reply_window_segment_ids: resolution
                    .map(|r| r.machine_segment_ids.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

fn ask_items(inputs: &DossierInputs) -> Vec<AskItem> {
    let at = inputs.as_of.at;
    let ledger: HashMap<i64, AskLedgerRef> = inputs
        .ledger
        .iter()
        .filter_map(|row| {
            row.request_id.map(|request_id| {
                (
                    request_id,
                    AskLedgerRef {
                        entry_id: row.entry_id,
                        kind_at: row.kind_at(at).to_owned(),
                        kind_source: row.kind_source.clone(),
                        keyword_kind: row.keyword_kind.clone(),
                        operator_turn_id: row.operator_turn_id,
                    },
                )
            })
        })
        .collect();
    genuine_stream(inputs)
        .into_iter()
        .map(|row| AskItem {
            request_id: row.request_id,
            at: row.at,
            text: row.text.clone(),
            turn_id: row.turn_id,
            ledger: ledger.get(&row.request_id).cloned(),
        })
        .collect()
}

pub fn full_view(inputs: &DossierInputs) -> DossierView {
    let prompts: HashMap<i64, &str> = inputs
        .stream
        .iter()
        .map(|row| (row.request_id, row.text.as_str()))
        .collect();
    let turn_texts: HashMap<i64, &str> = inputs
        .turns
        .iter()
        .map(|t| (t.turn_id, t.final_text.as_str()))
        .collect();
    DossierView {
        as_of: inputs.as_of,
        session: session_view(inputs),
        stream: stream_items(inputs),
        turns: turn_items(inputs),
        asks: ask_items(inputs),
        directives: inputs
            .entries
            .iter()
            .filter(|e| e.kind == "directive")
            .map(|e| directive_item(e, &prompts))
            .collect(),
        promises: inputs
            .entries
            .iter()
            .filter(|e| e.kind == "promise")
            .map(|e| promise_item(e, &turn_texts))
            .collect(),
        open_turn: open_turn_text(inputs),
        corrections: inputs.corrections.clone(),
        reactions: inputs.reactions.clone(),
        exclusions: inputs.exclusions.clone(),
    }
}

pub fn open_turn_text(inputs: &DossierInputs) -> Option<OpenTurnText> {
    let (turn_id, rows) = inputs.open_turn.as_ref()?;
    let mut text = String::new();
    let mut spans: Vec<(String, usize, usize)> = Vec::new();
    let mut offset = 0;
    for row in rows {
        let len = row.content.chars().count();
        match spans.last_mut() {
            Some(last) if last.0 == row.origin => last.2 = offset + len,
            _ => spans.push((row.origin.clone(), offset, offset + len)),
        }
        text.push_str(&row.content);
        offset += len;
    }
    Some(OpenTurnText {
        turn_id: *turn_id,
        text,
        spans,
        max_request_id: rows.iter().map(|r| r.request_id).max(),
    })
}
