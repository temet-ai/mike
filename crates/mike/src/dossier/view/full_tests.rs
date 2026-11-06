use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::json;

use super::full::full_view;
use super::types::{AsOf, DossierInputs};
use crate::store::persistence::dossier_entries::DossierEntryRow;
use crate::store::persistence::dossier_view::{DossierStreamRow, DossierTurnRow, SessionHeaderRow};

pub(crate) fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 20, 10, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

pub(crate) fn stream_row(
    request_id: i64,
    minute: i64,
    text: &str,
    kind: &str,
    turn_id: Option<i64>,
) -> DossierStreamRow {
    DossierStreamRow {
        request_id,
        at: base_time() + Duration::minutes(minute),
        text: text.to_owned(),
        kind_code: Some(kind.to_owned()),
        rule: None,
        repeat_key: None,
        classifier_version: Some("genuine-v1".to_owned()),
        turn_id,
        opened_by: turn_id.map(|_| "operator".to_owned()),
    }
}

pub(crate) fn turn_row(
    turn_id: i64,
    start_request_id: i64,
    minute: i64,
    prompt: &str,
    text: &str,
) -> DossierTurnRow {
    DossierTurnRow {
        turn_id,
        start_request_id,
        created_at: base_time() + Duration::minutes(minute),
        closed_at: Some(base_time() + Duration::minutes(minute + 5)),
        opened_by: Some("operator".to_owned()),
        prompt: prompt.to_owned(),
        final_text: text.to_owned(),
        files_touched_count: 0,
        text_chars: None,
        text_cut_at: None,
        text_spans: None,
        last_text_origin: None,
        stop_request_id: None,
        stop_reason: Some("end_turn".to_owned()),
        aborted: Some(false),
        interrupted: Some(false),
        close_cause: Some("next_prompt".to_owned()),
        checks_run: Some(json!([])),
        checks_count: Some(1),
        checks_failed: Some(0),
        ended_on: Some("report".to_owned()),
        ended_on_version: Some("ended-on-v1".to_owned()),
        builder_version: Some("turn-facts-v1".to_owned()),
        built_at: Some(base_time()),
        max_request_id: Some(start_request_id),
        session_class: Some("operator".to_owned()),
        session_class_rule: Some("default".to_owned()),
    }
}

pub(crate) fn directive(
    id: i64,
    request_id: i64,
    minute: i64,
    class: &str,
    polarity: &str,
    span: (usize, usize),
) -> DossierEntryRow {
    DossierEntryRow {
        id,
        session_id: 1,
        operator_turn_id: Some(request_id),
        kind: "directive".to_owned(),
        data: json!({"class": class, "polarity": polarity, "span": [span.0, span.1]}),
        status: "open".to_owned(),
        source: "capture".to_owned(),
        source_ref: format!("requests.requests:{request_id}"),
        dedup_key: format!("request:{request_id}:{class}:{polarity}"),
        extractor_version: "directives-v1".to_owned(),
        effective_at: base_time() + Duration::minutes(minute),
        recorded_at: base_time() + Duration::minutes(minute),
        closed_at: None,
        close_recorded_at: None,
        closed_by: None,
    }
}

pub(crate) fn fixture() -> DossierInputs {
    let prompts = [
        "Refactor the store. Then commit when green.",
        "Tool loaded.",
        "and add the docs",
        "thanks, next add a retry",
    ];
    let stream = vec![
        stream_row(10, 0, prompts[0], "operator", Some(1)),
        stream_row(11, 10, prompts[1], "tool-loaded", None),
        stream_row(12, 20, prompts[2], "operator", Some(2)),
        stream_row(13, 30, prompts[3], "operator", Some(3)),
    ];
    let turns = vec![
        turn_row(1, 10, 0, prompts[0], "Done. The store is refactored."),
        turn_row(
            2,
            12,
            20,
            prompts[2],
            "Docs written. I'll report back when CI finishes.",
        ),
        turn_row(3, 13, 30, prompts[3], "Retry added."),
    ];
    DossierInputs {
        session: SessionHeaderRow {
            session_db_id: 1,
            session_uuid: "00000000-0000-4000-8000-000000000001".to_owned(),
            project_path: Some("/repo".to_owned()),
            first_seen: base_time(),
        },
        as_of: AsOf::knowledge(base_time() + Duration::hours(2)),
        stream,
        turns,
        ledger: Vec::new(),
        entries: vec![directive(100, 10, 0, "commit", "allow", (25, 36))],
        goals: None,
        open_turn: None,
        corrections: Vec::new(),
        reactions: Vec::new(),
        exclusions: Vec::new(),
        steering_facts: None,
        semantic: None,
        notes: Vec::new(),
    }
}

#[test]
fn full_view_marks_machine_prompts_and_keeps_every_item_whole() {
    let inputs = fixture();
    let view = full_view(&inputs);
    assert_eq!(view.stream.len(), 4);
    let asks: Vec<i64> = view.asks.iter().map(|a| a.request_id).collect();
    assert_eq!(
        asks,
        vec![10, 12, 13],
        "the tool-loaded prompt is not an ask"
    );
    assert_eq!(view.turns[0].next.state, "resolved");
    assert_eq!(view.turns[0].next.request_id, Some(12));
    assert_eq!(view.turns[2].next.state, "pending");
}

#[test]
fn a_directive_carries_its_span_and_sentence() {
    let view = full_view(&fixture());
    let directive = &view.directives[0];
    assert_eq!(
        directive.sentence.as_deref(),
        Some("Then commit when green.")
    );
    assert_eq!(directive.text_span.as_deref(), Some("commit when"));
}
