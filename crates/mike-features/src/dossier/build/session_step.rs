use crate::store::persistence::dossier_repo::DossierPersist as _;
use std::collections::HashSet;

use chrono::{DateTime, Utc};

use super::BuildConfig;
use super::assemble::{Derived, FreshSources, RowInputs, assemble_row};
use super::load_step::{Fresh, Loaded, max_request_id, needs_fresh};
use super::resolve::{ResolveContext, TurnRow, resolve_selective};
use super::session_class::{
    ClassResult, LaneSummary, LaunchFacts, SessionClassInput, classify as classify_session,
    probe_candidate,
};
use crate::dossier::probe_point::Candidate;
use crate::store::persistence::turn_facts::{SessionRecord, TurnFactsRow, TurnRecord};
use crate::wire::genuine::PromptKind;
use crate::wire::genuine::stream::{StreamMessage, effective_kinds};

pub struct SessionRows {
    pub rows: Vec<TurnFactsRow>,
    pub skipped_stale: u64,
}

fn turn_rows(turns: &[&TurnRecord]) -> Vec<TurnRow> {
    turns
        .iter()
        .map(|turn| TurnRow {
            turn_id: turn.turn_id,
            start_request_id: turn.start_request_id,
            created_at: turn.created_at,
            closed_at: turn.closed_at,
            opened_by: turn.opened_by.clone(),
        })
        .collect()
}

fn later_genuine_turn_id(stream: &[StreamMessage], now: DateTime<Utc>) -> Option<i64> {
    let effective = effective_kinds(stream, now);
    stream
        .iter()
        .zip(effective)
        .filter(|(message, effective)| {
            message.turn_id.is_some()
                && effective.is_some_and(|effective| effective.kind == PromptKind::Genuine)
        })
        .nth(1)
        .and_then(|(message, _)| message.turn_id)
}

fn class_input(
    record: &SessionRecord,
    stream: &[StreamMessage],
    now: DateTime<Utc>,
) -> SessionClassInput {
    SessionClassInput {
        probe_verdict: crate::dossier::probe_point::verdict(record.session_id),
        is_template_session: record.is_template_session,
        launches: serde_json::from_value::<Vec<LaunchFacts>>(record.launches.clone())
            .unwrap_or_default(),
        project_path: record.project_path.clone(),
        first_prompt: record.first_prompt.clone(),
        lanes: LaneSummary {
            primary_without_agent_id: record.primary_without_agent_id,
            agent_id_requests: record.agent_id_requests,
            workflow_agent_requests: record.workflow_agent_requests,
        },
        role: record.role.clone(),
        later_genuine_turn_id: later_genuine_turn_id(stream, now),
    }
}

pub fn probe_candidates(
    loaded: &Loaded,
    session_ids: &[i64],
    now: DateTime<Utc>,
) -> Vec<Candidate> {
    let empty = Vec::new();
    session_ids
        .iter()
        .filter_map(|session_id| {
            let record = loaded.sessions.get(session_id)?;
            let stream = loaded.streams.get(session_id).unwrap_or(&empty);
            let input = class_input(record, stream, now);
            let reason = probe_candidate(&input)?;
            Some(Candidate {
                session_id: *session_id,
                first_prompt: input.first_prompt?,
                project_path: input.project_path,
                reason,
            })
        })
        .collect()
}

fn derived_for(loaded: &Loaded, fresh: &Fresh, turn: &TurnRecord) -> Option<Derived> {
    let existing = loaded.existing.get(&turn.turn_id);
    if !needs_fresh(loaded, turn) {
        return existing.map(Derived::from_existing);
    }
    let text = fresh.text.get(&turn.turn_id)?;
    let stop = fresh.stop.get(&turn.turn_id)?;
    if !text.consistent {
        return None;
    }
    let uses: Vec<_> = fresh
        .uses
        .get(&turn.turn_id)
        .map(|records| records.iter().collect())
        .unwrap_or_default();
    let final_text = fresh
        .final_text
        .get(&turn.turn_id)
        .map_or("", String::as_str);
    Some(Derived::from_fresh(&FreshSources {
        text,
        stop,
        uses: &uses,
        final_text,
        closed_at: turn.closed_at,
        max_request_id: max_request_id(loaded, turn),
    }))
}

pub fn build_session_rows(
    loaded: &Loaded,
    fresh: &Fresh,
    session_id: i64,
    now: DateTime<Utc>,
    cfg: &BuildConfig,
) -> SessionRows {
    let mut out = SessionRows {
        rows: Vec::new(),
        skipped_stale: 0,
    };
    let Some(record) = loaded.sessions.get(&session_id) else {
        return out;
    };
    let turns: Vec<&TurnRecord> = loaded
        .turns
        .iter()
        .filter(|turn| turn.session_id == session_id && turn.closed_at.is_some())
        .collect();
    let empty = Vec::new();
    let stream = loaded.streams.get(&session_id).unwrap_or(&empty);
    let ctx = ResolveContext {
        stream,
        last_seen: record.last_seen,
        now,
        settle: cfg.settle,
    };
    let new_rows: HashSet<i64> = turns
        .iter()
        .filter(|turn| !loaded.existing.contains_key(&turn.turn_id))
        .map(|turn| turn.turn_id)
        .collect();
    let resolved = resolve_selective(&ctx, &turn_rows(&turns), &new_rows);
    let class: ClassResult = classify_session(&class_input(record, stream, now));
    for turn in &turns {
        let Some(turn_resolved) = resolved.iter().find(|r| r.turn_id == turn.turn_id) else {
            continue;
        };
        let Some(derived) = derived_for(loaded, fresh, turn) else {
            tracing::warn!(
                turn_id = turn.turn_id,
                session_id,
                "turn facts skipped: the turn's captured sources are not yet consistent"
            );
            out.skipped_stale += 1;
            continue;
        };
        out.rows.push(assemble_row(&RowInputs {
            turn,
            resolved: turn_resolved,
            existing: loaded.existing.get(&turn.turn_id),
            derived,
            class: &class,
            now,
        }));
    }
    out
}

pub async fn classify_session_as_of(
    persist: &crate::store::persistence::PersistHandle,
    session_id: i64,
    as_of: DateTime<Utc>,
) -> Result<Option<ClassResult>, crate::store::persistence::PersistError> {
    let (streams, _) =
        super::load_step::load_streams_and_turns(persist, &[session_id], as_of).await?;
    let records = persist
        .dossier()
        .load_session_records(&[session_id])
        .await?;
    let Some(record) = records.iter().find(|r| r.session_id == session_id) else {
        return Ok(None);
    };
    let empty = Vec::new();
    let stream = streams.get(&session_id).unwrap_or(&empty);
    Ok(Some(classify_session(&class_input(record, stream, as_of))))
}
