use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{DateTime, Utc};

use super::checks::Missing;
use super::definition::FleetDefinition;
use super::events::FleetEvent;
use super::status_tasks::{criteria_status, task_status};

use super::plan::ordered_tasks;
pub use super::status_types::*;

const WORKING_WITHIN_SECONDS: i64 = 120;
const SILENT_AFTER_SECONDS: i64 = 1800;
const VERDICTS_SHOWN: usize = 6;
const QUIET_AFTER_SECONDS: i64 = 300;
const OPERATOR_STATUS: &str = "operator";
const SEATS_AT_WORK: [&str; 2] = ["working", "starting"];

pub fn build(
    header: FleetHeader,
    def: &FleetDefinition,
    seats: &[SeatFacts],
    notes: &[NoteFacts],
) -> FleetStatus {
    build_from_record(header, def, seats, notes, &[])
}

pub fn build_from_record(
    header: FleetHeader,
    def: &FleetDefinition,
    seats: &[SeatFacts],
    notes: &[NoteFacts],
    recorded: &[FleetEvent],
) -> FleetStatus {
    let evidence =
        super::acceptance::Evidence::new(&header.fleet, def, notes).with_judge_verdicts(recorded);
    build_with_evidence(header, def, (seats, notes), evidence, recorded)
}

fn build_with_evidence(
    header: FleetHeader,
    def: &FleetDefinition,
    (seats, notes): (&[SeatFacts], &[NoteFacts]),
    evidence: super::acceptance::Evidence<'_>,
    recorded: &[FleetEvent],
) -> FleetStatus {
    let verdicts = &evidence.verdicts;
    let seat_rows: Vec<SeatStatus> = seats
        .iter()
        .map(|s| seat_status(def, s, header.now))
        .collect();
    let mut events = recorded.to_vec();
    events.extend(super::fold::implied_seat_events(
        header.run_id.as_deref().unwrap_or_default(),
        def,
        seats,
        recorded,
    ));
    let folded = super::fold::fold(&events, DateTime::<Utc>::MAX_UTC);
    let tasks: Vec<TaskStatus> = ordered_tasks(def)
        .into_iter()
        .map(|(id, task)| task_status(id, task, &evidence, &seat_rows, &folded))
        .collect();
    let criteria = criteria_status(def, &tasks);
    let accepted = !criteria.is_empty()
        && criteria
            .iter()
            .all(|criterion| criterion.tasks > 0 && criterion.passed == criterion.tasks)
        && evidence
            .obligations
            .iter()
            .all(|obligation| !obligation.required || obligation.accepted());
    let blocked = notes
        .iter()
        .filter(|n| {
            n.kind == "task"
                && n.status.as_deref() == Some("blocked")
                && !n.tags.iter().any(|tag| tag == "acceptance-obligation")
        })
        .map(|n| Blocked {
            seat: n.seat.clone().unwrap_or_default(),
            title: n.title.clone(),
        })
        .collect();
    let needs_operator = notes
        .iter()
        .filter(|n| asks_the_operator(n))
        .map(|n| NeedsOperator {
            at: n.created_at,
            seat: n.seat.clone().unwrap_or_default(),
            title: n.title.clone(),
        })
        .collect();
    let tree_broken = super::shared_tree::state(&events, header.now);
    let state = folded.fleet_state(&header.state, accepted);
    let every_seat_idle_seconds = every_seat_idle(&state, &seat_rows, header.now);
    let newest = verdicts
        .iter()
        .rev()
        .take(VERDICTS_SHOWN)
        .cloned()
        .collect();
    FleetStatus {
        fleet: header.fleet,
        slug: header.slug,
        state,
        run_id: header.run_id,
        goal: def.goal.clone(),
        started_at: header.started_at,
        finished_at: header.finished_at,
        now: header.now,
        missing: header.missing,
        accepted,
        obligations: evidence.obligations,
        criteria,
        tasks,
        seats: seat_rows,
        verdicts: newest,
        blocked,
        needs_operator,
        every_seat_idle_seconds,
        tree_broken,
        problems: problems_of(recorded),
    }
}

fn problems_of(recorded: &[FleetEvent]) -> Vec<ProblemStatus> {
    let ledger = super::problems::Ledger::from_events(recorded);
    ledger
        .problems()
        .iter()
        .map(|problem| ProblemStatus {
            trigger: problem.trigger.clone(),
            task: problem.task.clone(),
            opened_at: problem.opened_at,
            detail: recorded
                .iter()
                .find(|e| {
                    e.payload.get("problem_id").and_then(|v| v.as_str())
                        == Some(problem.id.to_string().as_str())
                        && e.payload.get("detail").is_some()
                })
                .and_then(|e| e.payload.get("detail").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string(),
            decisions: problem.decisions.clone(),
            escalated: problem.escalated,
        })
        .collect()
}

fn asks_the_operator(note: &NoteFacts) -> bool {
    note.status.as_deref() == Some(OPERATOR_STATUS)
}

fn every_seat_idle(state: &str, seats: &[SeatStatus], now: DateTime<Utc>) -> Option<i64> {
    if state != "running" {
        return None;
    }
    let open: Vec<&SeatStatus> = seats.iter().filter(|s| !s.closed).collect();
    if open
        .iter()
        .any(|s| SEATS_AT_WORK.contains(&s.state.as_str()))
    {
        return None;
    }
    let newest = open.iter().filter_map(|s| s.last_request_at).max()?;
    let quiet = (now - newest).num_seconds();
    (quiet >= QUIET_AFTER_SECONDS).then_some(quiet)
}

const QUOTA_STATUS: i16 = 429;

fn seat_status(def: &FleetDefinition, facts: &SeatFacts, now: DateTime<Utc>) -> SeatStatus {
    let role = facts
        .role
        .clone()
        .or_else(|| facts.agent.clone())
        .unwrap_or_default();
    let seat = facts.seat.clone().unwrap_or_else(|| "unnamed".to_string());
    let goal = super::goals::seat_goal(def, &seat);
    SeatStatus {
        seat,
        role,
        model: facts.model.clone().unwrap_or_default(),
        state: seat_state(facts, now),
        session: facts.session,
        goal,
        context_tokens: facts.context_tokens,
        requests: facts.requests,
        last_request_at: facts.last_request_at,
        opened_at: facts.opened_at,
        closed: facts.closed_at.is_some(),
        first_request_cache: facts.first_request_cache,
    }
}

fn seat_state(facts: &SeatFacts, now: DateTime<Utc>) -> String {
    if facts.closed_at.is_some() {
        return "closed".to_string();
    }
    if facts.last_status == Some(QUOTA_STATUS) {
        return "quota".to_string();
    }
    match facts.live_state.as_deref() {
        Some("done") => return "idle".to_string(),
        Some(state) if state != "unknown" => return state.to_string(),
        _ => {}
    }
    let quiet = facts.last_request_at.map(|at| (now - at).num_seconds());
    match quiet {
        Some(seconds) if seconds < WORKING_WITHIN_SECONDS => "working",
        Some(seconds) if seconds > SILENT_AFTER_SECONDS => "silent",
        _ => "idle",
    }
    .to_string()
}

pub async fn assemble(
    state: &crate::state::AppState,
    row: &crate::store::persistence::fleet::FleetRow,
) -> Result<FleetStatus, String> {
    let persist = state
        .persist()
        .ok_or_else(|| "the database is not reachable".to_string())?;
    let def = FleetDefinition::from_value(&row.definition)?;
    let missing: Vec<Missing> = serde_json::from_value(row.missing.clone()).unwrap_or_default();
    let (seat_rows, note_rows) = match &row.run_id {
        Some(run) => (
            persist
                .fleet()
                .fleet_seat_rows(run)
                .await
                .map_err(|e| e.to_string())?,
            persist
                .fleet()
                .fleet_note_rows(run)
                .await
                .map_err(|e| e.to_string())?,
        ),
        None => (Vec::new(), Vec::new()),
    };
    let live = live_states(state).await;
    let seats: Vec<SeatFacts> = seat_rows
        .into_iter()
        .map(|s| SeatFacts {
            live_state: s.session_uuid.and_then(|id| live.get(&id).cloned()),
            session: s.session_uuid,
            seat: s.seat,
            role: s.role,
            agent: s.agent,
            model: s.model,
            opened_at: s.opened_at,
            closed_at: s.closed_at,
            requests: s.requests,
            last_request_at: s.last_request_at,
            context_tokens: s.context_tokens,
            last_status: s.last_status,
            first_request_cache: first_request_cache(
                s.forked,
                s.first_cache_read,
                s.first_cache_written,
            ),
        })
        .collect();
    let notes: Vec<NoteFacts> = note_rows.into_iter().map(note_facts).collect();
    let header = FleetHeader {
        fleet: row.fleet_uuid.to_string(),
        slug: row.slug.clone(),
        state: row.state.clone(),
        run_id: row.run_id.clone(),
        started_at: row.started_at,
        finished_at: row.finished_at,
        now: Utc::now(),
        missing,
    };
    let recorded = match &row.run_id {
        Some(run) => persist
            .fleet()
            .fleet_events_since(run, 0)
            .await
            .map_err(|e| e.to_string())?,
        None => Vec::new(),
    };
    let mut evidence = super::acceptance::Evidence::new(&header.fleet, &def, &notes)
        .with_judge_verdicts(&recorded);
    if let Some(run) = &row.run_id {
        super::ruling_proof::hydrate(persist, &def, run, &mut evidence.obligations).await;
    }
    Ok(build_with_evidence(
        header,
        &def,
        (&seats, &notes),
        evidence,
        &recorded,
    ))
}

pub fn note_facts(n: crate::store::persistence::fleet::FleetNoteRow) -> NoteFacts {
    NoteFacts {
        note_uuid: n.note_uuid,
        session_uuid: n.session_uuid,
        role: n.role,
        body: n.body,
        tags: n.tags,
        updated_at: n.updated_at,
        kind: n.kind,
        status: n.status,
        title: n.title,
        data: n.data,
        created_at: n.created_at,
        seat: n.seat,
    }
}

async fn live_states(
    state: &crate::state::AppState,
) -> std::collections::BTreeMap<uuid::Uuid, String> {
    let Ok(id) = state.config.launch.backend.parse() else {
        return Default::default();
    };
    let Some(backend) = crate::launch::backend_for(id) else {
        return Default::default();
    };
    match backend.list().await {
        Ok(surfaces) => surfaces
            .into_iter()
            .filter_map(|s| Some((s.session_uuid?, s.agent_state?)))
            .collect(),
        Err(_) => Default::default(),
    }
}
