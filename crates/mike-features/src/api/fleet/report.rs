use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::definition_view::DefinitionError;
use super::tree::running;
use crate::change_record::ChangeKind;
use crate::change_record::answers::check_after_last_change;
use crate::change_record::writers::{NoteFacts, change_record_enabled, record_note};
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::brief_notes::{fleet_tag, task_tag, task_tags};
use crate::launch::fleet::brief_notes_sync::latest_part;
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, ReportFiled, VerdictRecorded};
use crate::launch::fleet::floor;
use crate::launch::fleet::leaves;
use crate::launch::fleet::report::{
    CHECK_NO_COMMIT, CHECK_RECORD_OFF, CommitFacts, REPORT_KIND, REPORT_TAG, ReportRequest,
    acceptance_mail, acceptance_session, check_at_commit, check_report, newest_commit_of,
    report_body, report_data, report_title,
};
use crate::launch::fleet::seated::Seated;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{
    NewSessionNote, NoteOrder, SessionNoteFilter, SessionNoteRow,
};

const COMMIT_NOTES_READ: i64 = 100;
const CHECKS_READ: i64 = 100;
const FLOOR_TREE_DAYS: i64 = 7;
const COMMIT_KIND: &str = "result";
const COMMIT_TAG: &str = "commit";

#[derive(Debug, Serialize)]
pub struct ReportAnswer {
    note: Uuid,
    task: String,
    slice: String,
    criteria: Vec<String>,
    #[serde(flatten)]
    facts: CommitFacts,
    evidence: Vec<String>,
    told_acceptance: bool,
    acceptance_was_shown: String,
}

async fn newest_commit(persist: &PersistHandle, session: Uuid, task: &str) -> Option<String> {
    let filter = SessionNoteFilter {
        kind: Some(COMMIT_KIND),
        status: None,
        tag: Some(COMMIT_TAG),
        q: None,
        order: NoteOrder::NewestFirst,
        limit: COMMIT_NOTES_READ,
        offset: 0,
    };
    let rows = persist
        .launch()
        .fetch_session_notes(session, filter)
        .await
        .ok()?;
    let notes: Vec<_> = rows.into_iter().map(|r| (r.data, r.created_at)).collect();
    newest_commit_of(&notes, task)
}

async fn check_word(
    persist: &PersistHandle,
    checkout: &str,
    sha: Option<&str>,
    on: bool,
) -> String {
    let Some(sha) = sha else {
        return CHECK_NO_COMMIT.to_string();
    };
    if !on {
        return CHECK_RECORD_OFF.to_string();
    }
    match persist
        .fleet()
        .latest_change_events(checkout, &[], &[ChangeKind::Check], CHECKS_READ)
        .await
    {
        Ok(events) => check_at_commit(&events, sha).to_string(),
        Err(err) => {
            tracing::warn!(error = %err, "fleet report: could not read the check events");
            CHECK_RECORD_OFF.to_string()
        }
    }
}

pub async fn commit_facts(
    persist: &PersistHandle,
    (def, fleet, run): (&FleetDefinition, &str, &str),
    session: Uuid,
    (task, named): (&str, Option<&str>),
) -> Result<CommitFacts, String> {
    let own = newest_commit(persist, session, task).await;
    let commit = match named.filter(|sha| !sha.trim().is_empty()) {
        Some(sha) => Some(
            crate::launch::fleet::report_commit::named_commit(
                &def.repo,
                &def.branch,
                own.as_deref(),
                sha,
            )
            .await?,
        ),
        None => own,
    };
    let on = change_record_enabled(persist).await;
    let check = check_word(persist, &def.repo, commit.as_deref(), on).await;
    let checked_after_last_change = match persist.resolve_session_db_id(&session.to_string()).await
    {
        Ok(Some(id)) if on => check_after_last_change(persist, id, &def.repo).await.ok(),
        _ => None,
    };
    let brief_version = latest_part(persist, session, &task_tags(fleet, task))
        .await
        .map(|part| part.version);
    let mut proof = crate::launch::fleet::report_proof::collect(persist, def, run).await;
    proof.retain_task(task);
    match crate::launch::fleet::ruling_proof::collect(
        persist,
        (def, fleet, run),
        task,
        commit.as_deref(),
    )
    .await
    {
        Ok(rulings) => proof.rulings = rulings,
        Err(problem) => proof.rulings_unavailable = Some(problem),
    }
    Ok(CommitFacts {
        commit,
        check,
        checked_after_last_change,
        brief_version,
        check_command: crate::launch::fleet::operation::description(def, &def.check.command),
        proof,
    })
}

pub async fn record(persist: &PersistHandle, session: Uuid, note: &SessionNoteRow) {
    if !change_record_enabled(persist).await {
        return;
    }
    let facts = NoteFacts {
        note_uuid: note.note_uuid,
        note_kind: note.kind.clone(),
        title: note.title.clone(),
        body: note.body.clone(),
        status: note.status.clone(),
        data: note.data.clone(),
        at: Utc::now(),
    };
    if let Err(err) = record_note(persist, session, &facts).await {
        tracing::warn!(error = %err, note = %note.note_uuid, "fleet: could not record a report or verdict in the change record");
    }
}

pub fn filed_event(run: &str, task: &str, session: Uuid, note: Uuid) -> NewFleetEvent {
    NewFleetEvent {
        run_id: run.to_owned(),
        task: Some(task.to_owned()),
        session_uuid: Some(session),
        payload: EventPayload::ReportFiled(ReportFiled { note_uuid: note }),
    }
}

pub fn verdict_events(
    run: &str,
    task: &str,
    acceptance: Uuid,
    note: Uuid,
    criteria: &[String],
    passed: bool,
) -> Vec<NewFleetEvent> {
    criteria
        .iter()
        .map(|criterion| NewFleetEvent {
            run_id: run.to_owned(),
            task: Some(task.to_owned()),
            session_uuid: Some(acceptance),
            payload: EventPayload::VerdictRecorded(VerdictRecorded {
                note_uuid: note,
                criterion: criterion.clone(),
                passed,
                needed: String::new(),
            }),
        })
        .collect()
}

pub async fn append_all(persist: &PersistHandle, events: Vec<NewFleetEvent>, what: &str) {
    for event in events {
        if let Err(err) = persist.fleet().append_fleet_event(&event).await {
            tracing::warn!(error = %err, what, "fleet: could not record a fleet event");
        }
    }
}

async fn tell_acceptance(
    persist: &PersistHandle,
    def: &FleetDefinition,
    from: Uuid,
    shown: &str,
) -> bool {
    match acceptance_session(def) {
        Some(to) => persist
            .launch().insert_message(Uuid::new_v4(), to, Some(from), shown)
            .await
            .map_err(|err| tracing::warn!(error = %err, "fleet report: could not mail the acceptance seat"))
            .is_ok(),
        None => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Acceptance,
    Judge,
    NoCommit,
}

pub fn route(def: &FleetDefinition, seated: &Seated, sha: &str) -> Route {
    match (seated.acceptance_seated(def), sha.is_empty()) {
        (true, _) => Route::Acceptance,
        (false, true) => Route::NoCommit,
        (false, false) => Route::Judge,
    }
}

fn no_commit_mail(task: &str) -> String {
    format!(
        "[mike judge] Your report on task {task} names no commit, so there is nothing to judge: commit with fleet_commit and report again with the sha."
    )
}

fn spawn_floor(
    persist: PersistHandle,
    def: FleetDefinition,
    (run, task, seat, sha): (String, String, Uuid, String),
    tell: Option<String>,
) {
    tokio::spawn(async move {
        let input = floor::FloorInput {
            def: &def,
            run: &run,
            task: &task,
            seat,
            sha: &sha,
            since: Utc::now() - chrono::Duration::days(FLOOR_TREE_DAYS),
        };
        let passed = floor::run(&persist, &input)
            .await
            .is_none_or(|outcome| outcome.reaches_judge());
        if let (true, Some(shown)) = (passed, tell) {
            tell_acceptance(&persist, &def, seat, &shown).await;
        }
    });
}

async fn dispatch(
    persist: &PersistHandle,
    (def, seated): (&FleetDefinition, &Seated),
    (run, task, seat, sha): (&str, &str, Uuid, &str),
    (declared_floor, shown): (bool, &str),
) -> bool {
    let route = route(def, seated, sha);
    match route {
        Route::NoCommit => {
            leaves::tell_seat(persist, seat, &no_commit_mail(task)).await;
            false
        }
        Route::Acceptance if !declared_floor => tell_acceptance(persist, def, seat, shown).await,
        Route::Acceptance | Route::Judge => {
            if declared_floor {
                let tell = (route == Route::Acceptance).then(|| shown.to_string());
                spawn_floor(
                    persist.clone(),
                    def.clone(),
                    (run.to_string(), task.to_string(), seat, sha.to_string()),
                    tell,
                );
            }
            false
        }
    }
}

pub async fn report_slice(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<ReportRequest>,
) -> Result<Json<ReportAnswer>, DefinitionError> {
    let (row, def, run) = running(&state, &key).await?;
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let seated = Seated::of_run(persist, &run, &def).await;
    let (task, criteria) =
        check_report(&def, &seated, &req).map_err(DefinitionError::InvalidRequest)?;
    let session = req.session.ok_or(DefinitionError::Unavailable)?;
    let fleet = row.fleet_uuid.to_string();
    let facts = commit_facts(
        persist,
        (&def, &fleet, &run),
        session,
        (&req.task, req.sha.as_deref()),
    )
    .await
    .map_err(DefinitionError::InvalidRequest)?;
    let title = report_title(&req);
    let data = report_data(&req, &criteria, &facts);
    let body = format!(
        "{}\n\nCheck command: {}\n{}",
        report_body(&req),
        facts.check_command,
        facts.proof.body()
    );
    let tags = vec![
        fleet_tag(&fleet),
        REPORT_TAG.to_string(),
        task_tag(&req.task),
    ];
    let note = persist
        .launch()
        .create_session_note(
            session,
            NewSessionNote {
                kind: Some(REPORT_KIND),
                title: &title,
                body: Some(&body),
                status: None,
                tags: Some(&tags),
                data: Some(&data),
            },
        )
        .await?;
    record(persist, session, &note).await;
    append_all(
        persist,
        vec![filed_event(&run, &req.task, session, note.note_uuid)],
        "report_filed",
    )
    .await;
    let seat = format!("the seat of task {}", req.task);
    let shown = acceptance_mail(&seat, &req, &criteria, &facts, note.note_uuid);
    let told_acceptance = dispatch(
        persist,
        (&def, &seated),
        (
            &run,
            &req.task,
            session,
            facts.commit.as_deref().unwrap_or_default(),
        ),
        (task.done_when.is_some(), &shown),
    )
    .await;
    Ok(Json(ReportAnswer {
        note: note.note_uuid,
        task: req.task,
        slice: req.slice.trim().to_string(),
        criteria,
        facts,
        evidence: req.evidence,
        told_acceptance,
        acceptance_was_shown: shown,
    }))
}

#[cfg(test)]
#[path = "report_events_tests.rs"]
pub mod event_tests;
