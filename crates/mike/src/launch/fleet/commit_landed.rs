use uuid::Uuid;

use super::events::{CommitLanded, EventPayload, NewFleetEvent};
use crate::store::persistence::PersistHandle;

pub(super) async fn record(
    persist: &PersistHandle,
    run: &str,
    (task, session): (&str, Uuid),
    sha: &str,
) {
    let event = NewFleetEvent {
        run_id: run.to_string(),
        task: Some(task.to_string()),
        session_uuid: Some(session),
        payload: EventPayload::CommitLanded(CommitLanded {
            sha: sha.to_string(),
        }),
    };
    if let Err(err) = persist.append_fleet_event(&event).await {
        tracing::warn!(%err, task, sha, "fleet commit: could not record the landed commit");
    }
}
