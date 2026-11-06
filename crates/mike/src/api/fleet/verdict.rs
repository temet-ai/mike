use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use uuid::Uuid;

use super::definition_view::DefinitionError;
use super::report::{append_all, record, verdict_events};
use super::tree::running;
use crate::config::AppState;
use crate::launch::fleet::brief_notes::{fleet_tag, task_tag, task_tags};
use crate::launch::fleet::brief_notes_sync::latest_part;
use crate::launch::fleet::report::{
    DECISION_KIND, REPORT_KIND, VERDICT_TAG, VerdictRequest, check_verdict, decision_data,
    decision_title, seat_mail, task_session,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{
    NewSessionNote, NoteOrder, SessionNoteFilter, SessionNoteRow,
};

const REPORTS_READ: i64 = 200;

#[derive(Debug, Serialize)]
pub(crate) struct VerdictAnswer {
    note: Uuid,
    task: String,
    verdict: String,
    scope: String,
    criteria: Vec<String>,
    report_note: Option<Uuid>,
    brief_version: Option<i64>,
    told_seat: bool,
    told_session: Option<Uuid>,
    seat_closed: Option<Uuid>,
    seat_was_shown: String,
    regression_obligations: Vec<Uuid>,
}

async fn report_of(
    persist: &PersistHandle,
    worker: Uuid,
    task: &str,
    note: Uuid,
) -> Option<SessionNoteRow> {
    let tag = task_tag(task);
    let filter = SessionNoteFilter {
        kind: Some(REPORT_KIND),
        status: None,
        tag: Some(&tag),
        q: None,
        order: NoteOrder::NewestFirst,
        limit: REPORTS_READ,
        offset: 0,
    };
    let rows = persist.fetch_session_notes(worker, filter).await.ok()?;
    rows.into_iter().find(|row| row.note_uuid == note)
}

async fn brief_version(
    persist: &PersistHandle,
    (fleet, task): (&str, &str),
    worker: Option<Uuid>,
    report: Option<&SessionNoteRow>,
) -> Option<i64> {
    if let Some(version) = report.and_then(|r| r.data.get("brief_version")?.as_i64()) {
        return Some(version);
    }
    latest_part(persist, worker?, &task_tags(fleet, task))
        .await
        .map(|part| part.version)
}

pub(crate) async fn record_verdict(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<VerdictRequest>,
) -> Result<Json<VerdictAnswer>, DefinitionError> {
    let (row, def, run) = running(&state, &key).await?;
    let (task, judged) = check_verdict(&def, &req).map_err(DefinitionError::InvalidRequest)?;
    let acceptance = req.session.ok_or(DefinitionError::Unavailable)?;
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let fleet = row.fleet_uuid.to_string();
    let worker = task_session(task);
    let report = match (req.report_note, worker) {
        (Some(note), Some(worker)) => report_of(persist, worker, &req.task, note).await,
        _ => None,
    };
    if let (Some(note), None) = (req.report_note, &report) {
        return Err(DefinitionError::InvalidRequest(format!(
            "note {note} is not a report of task {} by the seat that holds it; pass the note id fleet_report returned",
            req.task
        )));
    }
    let version = brief_version(persist, (&fleet, &req.task), worker, report.as_ref()).await;
    let title = decision_title(&req.task, &judged, &req.reason);
    let data = decision_data(&req.task, &judged, (&req.reason, req.report_note, version));
    let tags = vec![
        fleet_tag(&fleet),
        VERDICT_TAG.to_string(),
        task_tag(&req.task),
    ];
    let regression_obligations =
        record_regressions(persist, acceptance, &fleet, &def, &req).await?;
    let note = persist
        .create_session_note(
            acceptance,
            NewSessionNote {
                kind: Some(DECISION_KIND),
                title: &title,
                body: Some(req.reason.trim()),
                status: None,
                tags: Some(&tags),
                data: Some(&data),
            },
        )
        .await?;
    record(persist, acceptance, &note).await;
    let events = verdict_events(
        &run,
        &req.task,
        acceptance,
        note.note_uuid,
        &judged.criteria,
        judged.verdict == "pass",
    );
    append_all(persist, events, "verdict_recorded").await;
    let shown = seat_mail(&req.task, &judged, &req.reason, req.report_note);
    let (told_session, seat_closed) = mail_verdict(persist, worker, acceptance, &shown).await;
    let told_seat = told_session.is_some();
    Ok(Json(VerdictAnswer {
        note: note.note_uuid,
        task: req.task,
        verdict: judged.verdict.to_string(),
        scope: judged.scope.to_string(),
        criteria: judged.criteria,
        report_note: req.report_note,
        brief_version: version,
        told_seat,
        told_session,
        seat_closed,
        seat_was_shown: if told_seat { shown } else { String::new() },
        regression_obligations,
    }))
}

async fn record_regressions(
    persist: &PersistHandle,
    acceptance: Uuid,
    fleet: &str,
    def: &crate::launch::fleet::FleetDefinition,
    req: &VerdictRequest,
) -> Result<Vec<Uuid>, DefinitionError> {
    let mut notes = Vec::new();
    for regression in &req.regressions {
        let title = format!("Regression for {} from {}", regression.task, req.task);
        let data = crate::launch::fleet::regression::data(req, regression);
        let tags = vec![
            fleet_tag(fleet),
            "acceptance-obligation".into(),
            task_tag(&regression.task),
        ];
        let note = persist
            .create_session_note(
                acceptance,
                NewSessionNote {
                    kind: Some("task"),
                    title: &title,
                    body: Some(&regression.reason),
                    status: Some("open"),
                    tags: Some(&tags),
                    data: Some(&data),
                },
            )
            .await?;
        record(persist, acceptance, &note).await;
        notes.push(note.note_uuid);
        let shown = format!(
            "[mike regression] Task {} owns required failed obligation {} from task {}.\nReason: {}\nFailing tests: {}\nEvidence: {}",
            regression.task,
            note.note_uuid,
            req.task,
            regression.reason,
            regression.failing_tests.join(", "),
            regression.evidence.join(", ")
        );
        let worker = def.tasks.get(&regression.task).and_then(task_session);
        mail_verdict(persist, worker, acceptance, &shown).await;
    }
    Ok(notes)
}

async fn mail_verdict(
    persist: &PersistHandle,
    worker: Option<Uuid>,
    acceptance: Uuid,
    shown: &str,
) -> (Option<Uuid>, Option<Uuid>) {
    let Some(worker) = worker else {
        return (None, None);
    };
    let closed = match persist.closed_fleet_seat(worker).await {
        Ok(closed) => closed,
        Err(err) => {
            tracing::warn!(%err, "fleet verdict: could not verify the task seat's state");
            return (None, None);
        }
    };
    let seat_closed = closed.as_ref().map(|_| worker);
    let to = match closed {
        Some(closed) => closed.successor,
        None => Some(worker),
    };
    let Some(to) = to else {
        return (None, seat_closed);
    };
    (
        send_verdict(persist, to, acceptance, shown).await,
        seat_closed,
    )
}

async fn send_verdict(
    persist: &PersistHandle,
    to: Uuid,
    acceptance: Uuid,
    shown: &str,
) -> Option<Uuid> {
    match persist
        .insert_message(Uuid::new_v4(), to, Some(acceptance), shown)
        .await
    {
        Ok(Some(_)) => Some(to),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(%err, "fleet verdict: could not mail the task's seat");
            None
        }
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use crate::store::persistence::launch::{NewLaunch, NewRun};
    use crate::store::persistence::test_support::acquire_ephemeral_db;

    #[tokio::test]
    async fn verdict_copy_reports_open_closed_and_missing_recipients_truthfully()
    -> anyhow::Result<()> {
        let Some(db) = acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let persist = PersistHandle::connect(&db.database_url).await?;
        let (acceptance, worker) = (Uuid::new_v4(), Uuid::new_v4());
        for session in [acceptance, worker] {
            persist.ensure_session_exists(&session.to_string()).await?;
        }
        let run = format!("verdict-mail-{}", Uuid::new_v4());
        persist
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: None,
                backend: "herdr".into(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await?;
        let mut launch = NewLaunch::new("herdr", "/tmp/verdict-mail");
        launch.run_id = Some(run);
        launch.session_uuid = Some(worker);
        let launch = persist.insert_launch(&launch).await?;
        assert_eq!(
            mail_verdict(&persist, Some(worker), acceptance, "pass").await,
            (Some(worker), None)
        );
        persist
            .record_launch_closed(launch, "complete", Some(true))
            .await?;
        assert_eq!(
            mail_verdict(&persist, Some(worker), acceptance, "pass").await,
            (None, Some(worker))
        );
        assert_eq!(
            mail_verdict(&persist, Some(Uuid::new_v4()), acceptance, "pass").await,
            (None, None)
        );
        assert_eq!(
            mail_verdict(&persist, None, acceptance, "pass").await,
            (None, None)
        );
        assert_eq!(
            persist
                .fetch_for_session_with_activity(worker)
                .await?
                .inbox
                .len(),
            1
        );
        db.cleanup().await?;
        Ok(())
    }
}
