use chrono::{DateTime, Utc};
use serde_json::Value;

use super::BUILDER_VERSION;
use super::checks::{BashResult, BashUse, Check, RegistryRow, derive_checks, failed_count};
use super::ended_on::{ENDED_ON_VERSION, TextSpan, non_sidecar_text};
use super::resolve::{AsOf, TurnResolved};
use super::session_class::ClassResult;
use crate::store::persistence::turn_facts::{
    BashUseRecord, StopFacts, TextFacts, TurnFactsRow, TurnRecord,
};

#[derive(Debug, Clone)]
pub struct Derived {
    pub max_request_id: i64,
    pub text_chars: i32,
    pub text_cut_at: i32,
    pub last_text_origin: Option<String>,
    pub text_spans: Value,
    pub stop_request_id: Option<i64>,
    pub stop_reason: Option<String>,
    pub aborted: bool,
    pub interrupted: bool,
    pub close_cause: String,
    pub checks_run: Value,
    pub checks_count: i32,
    pub checks_failed: i32,
    pub ended_on: String,
    pub ended_on_version: String,
}

pub struct FreshSources<'a> {
    pub text: &'a TextFacts,
    pub stop: &'a StopFacts,
    pub uses: &'a [&'a BashUseRecord],
    pub final_text: &'a str,
    pub closed_at: Option<DateTime<Utc>>,
    pub max_request_id: i64,
}

pub struct RowInputs<'a> {
    pub turn: &'a TurnRecord,
    pub resolved: &'a TurnResolved,
    pub existing: Option<&'a TurnFactsRow>,
    pub derived: Derived,
    pub class: &'a ClassResult,
    pub now: DateTime<Utc>,
}

impl Derived {
    pub fn from_existing(row: &TurnFactsRow) -> Self {
        Self {
            max_request_id: row.max_request_id,
            text_chars: row.text_chars,
            text_cut_at: row.text_cut_at,
            last_text_origin: row.last_text_origin.clone(),
            text_spans: row.text_spans.clone(),
            stop_request_id: row.stop_request_id,
            stop_reason: row.stop_reason.clone(),
            aborted: row.aborted,
            interrupted: row.interrupted,
            close_cause: row.close_cause.clone(),
            checks_run: row.checks_run.clone(),
            checks_count: row.checks_count,
            checks_failed: row.checks_failed,
            ended_on: row.ended_on.clone(),
            ended_on_version: row.ended_on_version.clone(),
        }
    }

    pub fn from_fresh(source: &FreshSources<'_>) -> Self {
        let checks = derive_checks(&bash_uses(source.uses), source.closed_at);
        let spans = parse_spans(&source.text.text_spans);
        let ended = non_sidecar_text(source.final_text, &spans).classify();
        let aborted = source.stop.stop_reason.as_deref() == Some("aborted")
            || source.stop.close_cause == "abort";
        Self {
            max_request_id: source.max_request_id,
            text_chars: source.text.text_chars,
            text_cut_at: source.text.text_cut_at,
            last_text_origin: source.text.last_text_origin.clone(),
            text_spans: source.text.text_spans.clone(),
            stop_request_id: source.stop.stop_request_id,
            stop_reason: source.stop.stop_reason.clone(),
            aborted,
            interrupted: source.stop.interrupted,
            close_cause: source.stop.close_cause.clone(),
            checks_run: checks_json(&checks),
            checks_count: i32::try_from(checks.len()).unwrap_or(i32::MAX),
            checks_failed: i32::try_from(failed_count(&checks)).unwrap_or(i32::MAX),
            ended_on: ended.class.code().to_string(),
            ended_on_version: ENDED_ON_VERSION.to_string(),
        }
    }
}

fn checks_json(checks: &[Check]) -> Value {
    serde_json::to_value(checks).unwrap_or_else(|_| Value::Array(Vec::new()))
}

pub fn parse_spans(value: &Value) -> Vec<TextSpan> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            Some(TextSpan {
                origin: item.get("origin")?.as_str()?.to_string(),
                start: usize::try_from(item.get("start")?.as_i64()?).ok()?,
                end: usize::try_from(item.get("end")?.as_i64()?).ok()?,
                first_request_id: item.get("first_request_id")?.as_i64()?,
                last_request_id: item.get("last_request_id")?.as_i64()?,
            })
        })
        .collect()
}

pub fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

fn bash_use(record: &BashUseRecord) -> BashUse {
    BashUse {
        tool_use_id: record.tool_use_id.clone(),
        request_id: record.request_id,
        at: record.at,
        command: record.command.clone(),
        result: record.result_content.as_ref().map(|content| BashResult {
            content: content_text(content),
            is_error: record.result_is_error.unwrap_or(false),
        }),
        registry: record.work_item_id.map(|work_item_id| RegistryRow {
            work_item_id,
            state: record.work_item_state.clone().unwrap_or_default(),
            exit_code: record.work_item_exit_code,
            finished_at: record.work_item_finished_at,
        }),
    }
}

fn bash_uses(records: &[&BashUseRecord]) -> Vec<BashUse> {
    records.iter().map(|record| bash_use(record)).collect()
}

fn skipped_json(as_of: &AsOf) -> Value {
    serde_json::to_value(&as_of.skipped).unwrap_or_else(|_| Value::Object(Default::default()))
}

pub fn assemble_row(inputs: &RowInputs<'_>) -> TurnFactsRow {
    let now = &inputs.resolved.now;
    let close = &inputs.resolved.at_close;
    let derived = &inputs.derived;
    let carried = inputs.existing;
    TurnFactsRow {
        operator_turn_id: inputs.turn.turn_id,
        session_id: inputs.turn.session_id,
        builder_version: BUILDER_VERSION.to_string(),
        built_at: inputs.now,
        max_request_id: derived.max_request_id,
        prompt_kind: now.kind.to_string(),
        prompt_rule: now.rule.map(str::to_string),
        prompt_kind_at_close: carried.map_or_else(
            || close.kind.to_string(),
            |row| row.prompt_kind_at_close.clone(),
        ),
        prompt_rule_at_close: carried.map_or_else(
            || close.rule.map(str::to_string),
            |row| row.prompt_rule_at_close.clone(),
        ),
        next_state: now.next_state.code().to_string(),
        next_genuine_request_id: now.next_genuine_request_id,
        next_genuine_turn_id: now.next_genuine_turn_id,
        next_genuine_at: now.next_genuine_at,
        next_state_at_close: carried.map_or_else(
            || close.next_state.code().to_string(),
            |row| row.next_state_at_close.clone(),
        ),
        next_genuine_request_id_at_close: carried.map_or(close.next_genuine_request_id, |row| {
            row.next_genuine_request_id_at_close
        }),
        skipped_before_next: skipped_json(now),
        reply_window_segment_ids: now.reply_window_segment_ids.clone(),
        reclassified_at: carried.and_then(|row| row.reclassified_at),
        text_chars: derived.text_chars,
        text_cut_at: derived.text_cut_at,
        last_text_origin: derived.last_text_origin.clone(),
        text_spans: derived.text_spans.clone(),
        stop_request_id: derived.stop_request_id,
        stop_reason: derived.stop_reason.clone(),
        aborted: derived.aborted,
        interrupted: derived.interrupted,
        close_cause: derived.close_cause.clone(),
        checks_run: derived.checks_run.clone(),
        checks_count: derived.checks_count,
        checks_failed: derived.checks_failed,
        ended_on: derived.ended_on.clone(),
        ended_on_version: derived.ended_on_version.clone(),
        session_class: inputs.class.class.code().to_string(),
        session_class_rule: inputs.class.rule.to_string(),
    }
}

pub fn materially_differs(old: &TurnFactsRow, new: &TurnFactsRow) -> bool {
    let mut aligned = new.clone();
    aligned.built_at = old.built_at;
    aligned != *old
}
