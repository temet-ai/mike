use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::retrospective::Job;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::work_threads::{HistoryPage, WorkThreadError};
use crate::store::persistence::{PersistError, PersistHandle};

const FINDING: &str = "finding";

pub fn problem(requests: i64, findings: usize, thread: &str) -> Option<String> {
    if requests <= 0 {
        return Some(
            "the retrospective session produced no turns: capture holds no request for its session, so no seat was debriefed and nothing was recorded"
                .to_string(),
        );
    }
    (findings == 0).then(|| {
        format!(
            "the retrospective session ran {requests} turns but wrote no `{FINDING}` entry on the `{thread}` work thread"
        )
    })
}

pub async fn verdict(
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    job: &Job,
) -> Result<Option<String>, PersistError> {
    let requests = turns(persist, job).await?;
    let findings = if requests > 0 {
        findings(persist, row, def, job.session).await?
    } else {
        0
    };
    Ok(problem(requests, findings, &def.retrospective.thread))
}

async fn turns(persist: &PersistHandle, job: &Job) -> Result<i64, PersistError> {
    Ok(persist
        .fleet()
        .fleet_seat_rows(&job.run_id)
        .await?
        .into_iter()
        .filter(|seat| seat.session_uuid == Some(job.session))
        .map(|seat| seat.requests)
        .sum())
}

pub(super) async fn findings(
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    session: Uuid,
) -> Result<usize, PersistError> {
    let mut paths = vec![row.project_path.as_str()];
    if def.repo != row.project_path {
        paths.push(def.repo.as_str());
    }
    let mut found = 0;
    for path in paths {
        found += findings_in(persist, path, &def.retrospective.thread, session).await?;
    }
    Ok(found)
}

async fn findings_in(
    persist: &PersistHandle,
    path: &str,
    thread: &str,
    session: Uuid,
) -> Result<usize, PersistError> {
    let Some(project) = persist
        .launch()
        .resolve_work_thread_project(path)
        .await
        .map_err(persist_error)?
    else {
        return Ok(0);
    };
    match persist
        .launch()
        .get_work_thread(project.id, thread, HistoryPage::everything())
        .await
    {
        Ok(detail) => Ok(detail
            .history
            .iter()
            .filter(|entry| entry.kind == FINDING && entry.session_uuid == Some(session))
            .count()),
        Err(WorkThreadError::NotFound(_)) => Ok(0),
        Err(other) => Err(persist_error(other)),
    }
}

fn persist_error(error: WorkThreadError) -> PersistError {
    match error {
        WorkThreadError::Persist(inner) => inner,
        other => PersistError::DataInvariant(other.to_string()),
    }
}

pub fn mail(row: &FleetRow, def: &FleetDefinition, problem: &str) -> String {
    format!(
        "[mike] The retrospective of fleet {} ended failed: {problem}. Nothing from this run reached the `{}` work thread, so the next define_fleet will not show it; debrief the seats and record the findings by hand.",
        row.slug, def.retrospective.thread
    )
}

pub async fn tell(persist: &PersistHandle, row: &FleetRow, def: &FleetDefinition, problem: &str) {
    let Some(author) = row.created_by else {
        tracing::warn!(fleet = %row.slug, problem, "fleet: the retrospective failed and the fleet records no author to tell");
        return;
    };
    let sent = persist
        .launch()
        .insert_message(Uuid::new_v4(), author, None, &mail(row, def, problem))
        .await;
    if let Err(err) = sent {
        tracing::warn!(fleet = %row.slug, error = %err, "fleet: could not mail the failed retrospective");
    }
}
