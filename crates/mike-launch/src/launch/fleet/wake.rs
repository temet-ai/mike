use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::told;
use crate::base_state::BaseState;
use crate::change_record::types::{ChangeKind, NewChangeEvent};
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;

const SILENT_SECONDS: i64 = 300;
pub const SEAT_WOKEN: &str = "seat_woken";

pub async fn pass(state: &BaseState, persist: &PersistHandle, def: &FleetDefinition, run: &str) {
    let now = Utc::now();
    for (task, session) in seats(def) {
        let Ok(Some(db_id)) = persist.resolve_session_db_id(&session.to_string()).await else {
            continue;
        };
        let Ok(counts) = persist.launch().fetch_inbox_counts(db_id).await else {
            continue;
        };
        if counts.unconsumed == 0 {
            continue;
        }
        let last = persist.session_last_request_at(db_id).await.ok().flatten();
        if !silent(last, now) {
            continue;
        }
        let key = wake_key(session, last);
        if told::already(persist, run, &key).await {
            continue;
        }
        let reminder = wake_reminder(counts.unconsumed);
        if state
            .mail_push()
            .push_via_remote_control(session, &reminder)
            .await
        {
            told::mark(persist, run, &[key]).await;
            record(persist, def, run, &task, session, counts.unconsumed).await;
        }
    }
}

fn seats(def: &FleetDefinition) -> Vec<(String, Uuid)> {
    def.tasks
        .iter()
        .filter_map(|(task, seat)| {
            let session = Uuid::parse_str(seat.session.as_deref()?).ok()?;
            Some((task.clone(), session))
        })
        .collect()
}

pub fn silent(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    last.is_none_or(|at| now - at >= Duration::seconds(SILENT_SECONDS))
}

pub fn wake_key(session: Uuid, last: Option<DateTime<Utc>>) -> String {
    let since = last.map(|at| at.to_rfc3339()).unwrap_or_default();
    format!("wake:{session}:{since}")
}

fn wake_reminder(unread: i64) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::SessionMailbox {
            message_uuid: Uuid::new_v4(),
            from_session_uuid: None,
        },
        payload: format!(
            "You have {unread} unread mike message(s) and have been silent for five minutes. Read your mailbox with workspace_heartbeat or get_session_messages, then carry on with your task."
        ),
    }
}

#[cfg(test)]
#[path = "wake_tests.rs"]
mod tests;

async fn record(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    task: &str,
    session: Uuid,
    unread: i64,
) {
    let Ok(Some(project_id)) = persist.find_project_id_by_path(&def.repo).await else {
        return;
    };
    let session_id = persist
        .resolve_session_db_id(&session.to_string())
        .await
        .ok()
        .flatten();
    let event = NewChangeEvent {
        project_id,
        checkout: def.repo.clone(),
        at: Utc::now(),
        kind: ChangeKind::Mail,
        session_id,
        run_id: Some(run.to_string()),
        task: Some(task.to_string()),
        paths: Vec::new(),
        subject: SEAT_WOKEN.to_string(),
        detail: serde_json::json!({ "session": session.to_string(), "unread": unread }),
        before_sha: None,
        after_sha: None,
    };
    if let Err(err) = persist.fleet().insert_change_events(&[event]).await {
        tracing::warn!(%run, error = %err, "mike could not record a seat wake in the change record");
    }
}
