use super::*;
use crate::launch::fleet::brief_notes::task_tag;
use crate::launch::fleet::report::{REPORT_KIND, REPORT_TAG};
use crate::store::persistence::fleet::FleetNoteRow;

const REPORTS_PER_TASK: usize = 3;

pub(super) async fn add(persist: &PersistHandle, run: &str, routes: &mut Routes) {
    if routes.tasks.is_empty() {
        return;
    }
    let notes = persist.fleet().fleet_note_rows(run).await;
    for (task, session) in &routes.tasks {
        let context = match &notes {
            Ok(notes) => references(task, notes),
            Err(_) => format!(
                "Retained task {task} reports could not be read; use list_session_notes with tag {} in this fleet before assigning bounded rework. Missing context is not evidence of absent reports.",
                task_tag(task)
            ),
        };
        match session {
            Some(session) => {
                routes.owners.entry(*session).or_default().insert(context);
            }
            None => {
                routes.unresolved.insert(context);
            }
        }
    }
    if let Err(error) = notes {
        tracing::warn!(%run, %error, "fleet: could not read retained reports for gate failure handoff");
    }
}

fn references(task: &str, notes: &[FleetNoteRow]) -> String {
    let tag = task_tag(task);
    let mut reports: Vec<_> = notes
        .iter()
        .filter(|note| {
            note.kind == REPORT_KIND
                && note.tags.iter().any(|tag| tag == REPORT_TAG)
                && note.tags.contains(&tag)
                && note.data.get("task").and_then(|v| v.as_str()) == Some(task)
        })
        .collect();
    reports.sort_by_key(|note| std::cmp::Reverse((note.created_at, note.note_uuid)));
    let mut lines = vec![format!(
        "Retained task {task} context: reports retain their recorded summary, scenario, check command, proof and evidence where provided. Read the referenced notes before assigning bounded rework; a report is not evidence of causation or a passing check at this gate commit. Do not reopen or keep the original seat waiting for the global gate."
    )];
    if reports.is_empty() {
        lines.push("No retained report was found for this task in this run; recover the task brief and establish missing evidence before assigning rework.".into());
    }
    for note in reports.iter().take(REPORTS_PER_TASK) {
        let commit = note.data.get("commit").and_then(|v| v.as_str());
        let commit = commit.filter(|sha| valid_commit(sha)).unwrap_or("unknown");
        lines.push(format!(
            "Report note {}, original session {}, reported commit {commit}; list_session_notes(session_id={}, kind=report, tag={tag}).",
            note.note_uuid, note.session_uuid, note.session_uuid
        ));
    }
    let omitted = reports.len().saturating_sub(REPORTS_PER_TASK);
    if omitted > 0 {
        lines.push(format!(
            "{omitted} older reports omitted; list_session_notes with tag {tag} in this fleet retains the full task history, including closed sessions."
        ));
    }
    lines.join("\n")
}

fn valid_commit(commit: &str) -> bool {
    (7..=64).contains(&commit.len()) && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
}
