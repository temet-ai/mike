use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::session_notes::NewSessionNote;

const SEND_READY_AGAIN: &str = "ready again";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Notice {
    pub to: Uuid,
    pub mail: String,
    pub note_title: String,
    pub note_body: String,
    pub tag: String,
}

fn reason_line(problem: &str) -> String {
    problem
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no reason was recorded")
        .trim_end_matches(':')
        .to_string()
}

fn fix(problem: &str) -> &'static str {
    let lower = problem.to_lowercase();
    if lower.contains(SEND_READY_AGAIN) {
        ""
    } else if lower.contains("verified baseline") || lower.contains(":unverified") {
        " Fix: read the tree-check result it names, make gate.command run in a clean checkout of the branch, then send ready again (define_fleet ready: true)."
    } else if lower.contains("baseline checkout has changes") {
        " Fix: commit or move the changes in the checkout so the branch head is clean, then send ready again (define_fleet ready: true)."
    } else if lower.contains("database is not reachable") {
        " Fix: start the database mike uses, then send ready again (define_fleet ready: true)."
    } else {
        " Fix: correct what the reason names, then send ready again (define_fleet ready: true)."
    }
}

pub(super) fn notice(row: &FleetRow, problem: &str) -> Option<Notice> {
    let to = row.created_by?;
    let reason = reason_line(problem);
    let mail = format!(
        "[mike] Fleet {} did not start: {reason}.{}",
        row.slug,
        fix(problem)
    );
    Some(Notice {
        to,
        note_title: format!(
            "Fleet {} did not start: fix it and send ready again",
            row.slug
        ),
        note_body: format!(
            "{mail}\n\nThe whole reason, as recorded in the fleet's start_report:\n{problem}"
        ),
        mail,
        tag: format!("fleet:{}", row.fleet_uuid),
    })
}

pub(super) async fn tell(persist: &PersistHandle, row: &FleetRow, problem: &str) {
    match notice(row, problem) {
        Some(notice) => {
            mail(persist, &row.slug, &notice).await;
            raise(persist, &row.slug, &notice).await;
        }
        None => nobody_to_tell(&row.slug),
    }
}

fn nobody_to_tell(slug: &str) {
    tracing::warn!(fleet = %slug, "fleet: start failed and no session sent ready, so nobody was told");
}

async fn mail(persist: &PersistHandle, slug: &str, notice: &Notice) {
    let sent = persist
        .insert_message(Uuid::new_v4(), notice.to, None, &notice.mail)
        .await;
    if let Err(err) = sent {
        tracing::warn!(fleet = %slug, error = %err, "fleet: could not mail the failed start");
    }
}

async fn raise(persist: &PersistHandle, slug: &str, notice: &Notice) {
    let tags = [notice.tag.clone()];
    let note = NewSessionNote {
        kind: Some("note"),
        title: &notice.note_title,
        body: Some(&notice.note_body),
        status: Some("operator"),
        tags: Some(&tags),
        data: None,
    };
    if let Err(err) = persist.create_session_note(notice.to, note).await {
        tracing::warn!(fleet = %slug, error = %err, "fleet: could not raise the failed start for the operator");
    }
}

#[cfg(test)]
#[path = "start_failure_tests.rs"]
mod tests;
